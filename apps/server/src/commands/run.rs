//! A run, as a session at the terminal or one `exec` task: all is checked and read on the main
//! thread first (workspace, schema, key, prompt), then the session is composed: a session with its
//! interface in this process, over channels to its server on a thread. Each task is run, reported
//! and recorded; Ctrl-C stops it (see `interrupt`), not the process.

mod ended;
mod front;
mod interface;
mod task;

use std::any::Any;
use std::io::IsTerminal;
use std::path::Path;
use std::sync::mpsc::Receiver;

use contracts::history::{History, Session};
use tools::workspace::Workspace;

use crate::attach;
use crate::choice;
use crate::cli::RunOptions;
use crate::commands::sessions;
use crate::compose::{Seat, Setup, Shared, compose, lane};
use crate::context::{self, Answer, Environment};
use crate::desk;
use crate::error::Failure;
use crate::gate::Tty;
use crate::input::{self, Prompt};
use crate::interrupt;
use crate::mode;
use crate::modes::Modes;
use crate::rules::Rules;
use crate::server::Ui;

pub(crate) use self::ended::Ended;
pub use self::front::{Front, Unready};
#[cfg(any(test, feature = "testing"))]
pub(crate) use self::interface::serving;

/// The workspace `options` name (`-C`, else the current directory), where the sessions work.
pub(crate) fn root(options: &RunOptions) -> Result<Workspace, Failure> {
    let root = options.cd.as_deref().unwrap_or_else(|| Path::new("."));
    workspace(root).map_err(|why| Failure::Usage(format!("-C {}: {why}", root.display())))
}

/// The workspace at `dir`, with llx's config (keys, tokens) and sessions off limits to the tools:
/// a prompt injection must not read them.
fn workspace(dir: &Path) -> Result<Workspace, String> {
    let workspace = Workspace::new(dir).map_err(|error| error.to_string())?;
    if !workspace.root().is_dir() {
        return Err("not a directory".into());
    }
    let private = provider_kit::store::dir(crate::compose::APP).into_iter();
    let sessions = sessions::store().dir;
    Ok(workspace.denying(private.chain([sessions])))
}

/// How a run takes its tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A session at the terminal: the prompt, when given, is its first message.
    Session,
    /// One task, then exit (`exec`, -p).
    Exec,
}

/// What a run does once composed.
#[derive(Debug, PartialEq, Eq)]
enum Work {
    /// One task, then exit.
    One(String),
    /// A session in the interface, after a first message when there is one.
    Session(Option<String>),
}

/// What a session needs that `exec` does not, as a usage error.
const SESSION_TERMINAL: &str = "a session needs a terminal on stdin: run one task with \
`llx exec \"<task>\"` (or -p), or one more in a saved session with `llx exec resume`";
const NO_INTERFACE: &str = "a session's interface cannot start at this terminal";
const RUN_EXEC: &str = "run one task with `llx exec \"<task>\"` (or -p)";
const ONE_TASK: &str = "--json, --schema and -o answer one task: run it with `llx exec` (or -p)";
const EXEC_PROMPT: &str = "exec runs one task: give it as the prompt, or `-` to read it from stdin";

/// The work `kind` asks of `prompt`; stdin is read only for one task (a session's is the
/// terminal its interface reads).
fn work(prompt: Option<&str>, kind: Kind) -> Result<Work, Failure> {
    match kind {
        Kind::Exec => match input::prompt(prompt)? {
            Prompt::One(task) => Ok(Work::One(task)),
            Prompt::Line => Err(Failure::Usage(EXEC_PROMPT.into())),
        },
        Kind::Session => Ok(Work::Session(prompt.map(str::to_string))),
    }
}

/// Whether a session can run: at a terminal, with no option that answers one task.
fn session_allowed(
    prompt: Option<&str>,
    options: &RunOptions,
    terminal: bool,
) -> Result<(), Failure> {
    if options.json || options.schema.is_some() || options.output_last_message.is_some() {
        return Err(Failure::Usage(ONE_TASK.into()));
    }
    if !terminal || prompt == Some("-") {
        return Err(Failure::Usage(SESSION_TERMINAL.into()));
    }
    Ok(())
}

/// Run `prompt` with `options` as `kind`, in `resumed` when a session goes on, its session shown
/// at this terminal by the binary's `front`.
pub(crate) fn run(
    prompt: Option<&str>,
    options: RunOptions,
    resumed: Option<Session>,
    kind: Kind,
    front: &'static dyn Front,
) -> Result<i32, Failure> {
    let workspace = root(&options)?;
    let schema = options.schema.as_deref().map(input::schema).transpose()?;
    let rules = Rules::new(&options.allow, &options.deny).map_err(Failure::Usage)?;
    // A picture asked for that cannot be attached stops the run before a model is paid for.
    for path in &options.images {
        attach::read(path).map_err(|why| Failure::Usage(format!("--image {why}")))?;
    }
    if kind == Kind::Session {
        session_allowed(prompt, &options, std::io::stdin().is_terminal())?;
    }
    let choice = choice::resolve(&options.models, choice::saved(), None)?;
    // A session starts signed out too, so that /login can sign in; one task cannot.
    if kind == Kind::Exec && choice.provider.signed_in().is_none() {
        return Err(Failure::Environment(choice.provider.how_to_sign_in()));
    }
    let work = work(prompt, kind)?;
    let modes = Modes::load()?.with(&rules);
    let mode = modes.choose(options.mode.as_deref(), options.yes)?;
    let (terminal, asks) = (Tty::available(), kind == Kind::Session);
    let path = std::env::var_os("PATH").unwrap_or_default();
    // An interface draws a session's answers, and one task's on a terminal's stdout.
    let drawn = kind == Kind::Session
        || (std::io::stdout().is_terminal() && !options.json && options.schema.is_none());
    let environment = Environment {
        root: workspace.root(),
        mode: &mode::describe(&mode, terminal, asks),
        now: crate::clock::now(),
        runtimes: &context::runtimes(&path),
        answer: if drawn { Answer::Drawn } else { Answer::Plain },
    };
    let fresh = context::gather(&environment);
    let context = match &resumed {
        Some(session) => context::resumed(&fresh, session, workspace.root()),
        None => fresh.clone(),
    };
    let history = sessions::store();
    let mut session = resumed.unwrap_or_else(|| history.start());
    if session.workspace.is_empty() {
        session.workspace = workspace.root().display().to_string();
    }
    // Before the terminal is taken: a session another llx has open is not opened here (K8).
    let record = history
        .hold(&session)
        .map_err(|why| Failure::Environment(why.to_string()))?;
    // Then the session's interface: what it holds of the terminal is given back at the end.
    let ready = match work {
        Work::One(task) => Ready::One(task),
        Work::Session(first) => Ready::Session(first, interface::in_process(&workspace, front)?),
    };
    let setup = Setup {
        options,
        choice,
        workspace,
        context,
        fresh,
        schema,
        mode,
        modes,
        terminal,
        asks,
        history,
        session,
        record,
        drawing: Some(front.drawing()),
    };
    interrupt::install();
    Ok(ready.run(setup, front))
}

/// A run's work with what it runs in: a session with its interface, or one task.
enum Ready {
    Session(Option<String>, Attached),
    One(String),
}

/// A session's interface attached to its server: what holds this process's terminal while the
/// session lasts, given back when it drops; the sessions' port, and what the person asks of the
/// desk.
struct Attached {
    held: Box<dyn Any>,
    ui: Ui,
    requests: Receiver<desk::Request>,
}

impl Ready {
    /// Run it, `front` the binary that holds the terminal; the exit code.
    fn run(self, setup: Setup, front: &'static dyn Front) -> i32 {
        match self {
            // Each lane of the session is a session of its own, on its own thread.
            Self::Session(first, attached) => {
                let Attached { held, ui, requests } = attached;
                let (shared, seat) = setup.split();
                let seat = Seat {
                    ui: Some(ui.clone()),
                    ..seat
                };
                let start = |shared: &Shared, seat, work: desk::Work| lane(shared, seat, work);
                // Lanes that do not stop in time: the terminal goes back, and the process ends.
                let quit = || front.quit();
                desk::run(&shared, &ui, requests, seat, first, &start, &quit);
                // The terminal goes back once the session is over, its farewell after.
                drop(held);
                crate::exit::OK
            }
            Self::One(task) => compose(setup, |rig| {
                rig.attached = rig.shared.options.images.clone();
                rig.task(&task)
            }),
        }
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
