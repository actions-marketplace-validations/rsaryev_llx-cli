//! A run, as a session at the terminal or one `exec` task: all is checked and read on the main
//! thread first (workspace, schema, key, prompt), then the session is composed. A session at the
//! terminal is the daemon's (`llx serve`, docs/daemon.md) unless `--no-daemon`: this process is
//! then its client, and the daemon runs the same code for it (`At::Served`). Each task is run,
//! reported and recorded; Ctrl-C stops it (see `interrupt`), not the process.

mod ended;
mod front;
mod interface;
mod task;

use crate::rules::Rules;
use std::any::Any;
use std::io::IsTerminal;
use std::path::Path;
use std::sync::mpsc::Receiver;

use contracts::history::{History, Session};
use protocol::Place;
use tools::workspace::Workspace;

use crate::attach;
use crate::choice;
use crate::cli::RunOptions;
use crate::commands::sessions;
use crate::compose::{Seat, Setup, Shared, compose, lane};
use crate::context::{self, Environment};
use crate::desk;
use crate::error::Failure;
use crate::gate::Tty;
use crate::input::{self, Prompt};
use crate::interrupt;
use crate::mode;
use crate::modes::Modes;
use crate::server::{Ui, place};

pub(crate) use self::ended::Ended;
pub use self::front::{Front, Unready};
pub(crate) use self::interface::{Left, info, serving};

/// The workspace `options` name (`-C`, else the current directory), with what is llx's own off
/// limits to the tools (`place::at`).
pub(crate) fn root(options: &RunOptions) -> Result<Workspace, Failure> {
    let root = options.cd.as_deref().unwrap_or_else(|| Path::new("."));
    place::at(root).map_err(|why| Failure::Usage(format!("-C {}: {why}", root.display())))
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

/// Where a run's session is shown.
pub(crate) enum At {
    /// At this process's terminal: through the daemon, else in this process, as the binary
    /// shows it.
    Terminal(&'static dyn Front),
    /// Served by the daemon to the client that attached from its place.
    Served(Served),
}

impl At {
    /// The binary's side of the run, wherever it is shown.
    pub(crate) fn front(&self) -> &'static dyn Front {
        match self {
            Self::Terminal(front) => *front,
            Self::Served(served) => served.front,
        }
    }
}

/// A session the daemon serves: where its client is, the client's interface, what it asks of
/// the desk, and the binary that runs the daemon (its lanes' trace is drawn as it draws one).
pub(crate) struct Served {
    pub place: Place,
    pub ui: Ui,
    pub requests: Receiver<desk::Request>,
    pub front: &'static dyn Front,
}

/// Run `prompt` with `options` as `kind`, in `resumed` when a session goes on, shown `at`.
pub(crate) fn run(
    prompt: Option<&str>,
    options: RunOptions,
    resumed: Option<Session>,
    kind: Kind,
    at: At,
) -> Result<i32, Failure> {
    let mut workspace = root(&options)?;
    let schema = options.schema.as_deref().map(input::schema).transpose()?;
    let rules = Rules::new(&options.allow, &options.deny).map_err(Failure::Usage)?;
    // A picture asked for that cannot be attached stops the run before a model is paid for.
    for path in &options.images {
        attach::read(path).map_err(|why| Failure::Usage(format!("--image {why}")))?;
    }
    let front = at.front();
    let served = match at {
        At::Terminal(_) => None,
        At::Served(served) => Some(served),
    };
    if kind == Kind::Session && served.is_none() {
        session_allowed(prompt, &options, std::io::stdin().is_terminal())?;
    }
    // Served, the session thinks with its client's keys, not the daemon's.
    let vars = served.as_ref().map(|served| served.place.env.as_slice());
    let choice = choice::resolve(&options.models, choice::saved(), vars)?;
    // A session starts signed out too, so that /login can sign in; one task cannot.
    if kind == Kind::Exec && choice.provider.signed_in().is_none() {
        return Err(Failure::Environment(choice.provider.how_to_sign_in()));
    }
    let work = work(prompt, kind)?;
    let modes = Modes::load()?.with(&rules);
    let mode = modes.choose(options.mode.as_deref(), options.yes)?;
    // The daemon's client has the terminal the person is asked at.
    let (terminal, asks) = (served.is_some() || Tty::available(), kind == Kind::Session);
    let in_daemon = served.is_some();
    // A session's interface attaches first: its sessions work where it is.
    let (ready, place) = match (work, served) {
        (Work::One(_), Some(_)) => return Err(Failure::Usage(RUN_EXEC.into())),
        (Work::One(task), None) => (Ready::One(task), None),
        (Work::Session(first), Some(served)) => {
            let attached = Attached {
                held: None,
                ui: served.ui,
                requests: served.requests,
            };
            (Ready::Session(first, attached), Some(served.place))
        }
        (Work::Session(first), None) => {
            if !options.no_daemon {
                match front.remote(workspace.root()) {
                    Ok(()) => return Ok(crate::exit::OK),
                    Err(why) => {
                        log::warn!("{why}: {IN_PROCESS}");
                        crate::error::note(&format!("llx: {why}: {IN_PROCESS}\n"));
                    }
                }
            }
            let (attached, place) = interface::in_process(&workspace, front)?;
            (Ready::Session(first, attached), Some(place))
        }
    };
    if let Some(place) = &place {
        workspace = place::workspace(place).map_err(Failure::Usage)?;
    }
    // The client's `PATH` in the daemon, as its commands run with it.
    let path = (workspace
        .env()
        .and_then(|env| env.iter().find(|(name, _)| name == "PATH")))
    .map_or_else(
        || std::env::var_os("PATH").unwrap_or_default(),
        |(_, path)| path.into(),
    );
    let environment = Environment {
        root: workspace.root(),
        mode: &mode::describe(&mode, terminal, asks),
        now: crate::clock::now(),
        runtimes: &context::runtimes(&path),
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
        drawing: Some(front.drawing()),
    };
    // The daemon's signals are its own: a served session installs none.
    if !in_daemon {
        interrupt::install();
    }
    Ok(ready.run(setup, front))
}

/// Said when the daemon cannot serve this session.
const IN_PROCESS: &str = "this session runs in this process";

/// A run's work with what it runs in: a session with its interface, or one task.
enum Ready {
    Session(Option<String>, Attached),
    One(String),
}

/// A session's interface attached to its server: what holds this process's terminal while a
/// session in process lasts (none when the daemon serves a client), given back when it drops;
/// the sessions' port, and what the person asks of the desk.
struct Attached {
    held: Option<Box<dyn Any>>,
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
                // Lanes that do not stop in time: the terminal goes back, and the process ends;
                // in the daemon, which serves other terminals, the client is told the session is
                // over: its lanes end as they can,
                // with nobody waiting on them.
                let in_process = held.is_some();
                let told = ui.clone();
                let quit = move || match in_process {
                    true => front.quit(),
                    false => told.send(protocol::Event::Quit),
                };
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
