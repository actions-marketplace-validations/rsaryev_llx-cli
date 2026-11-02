//! `llx serve`: the daemon (docs/daemon.md). It takes the lock of its config, binds the socket,
//! says it is ready, and accepts clients until it is stopped; each client that attaches gets a
//! desk of its own, built as a terminal's session is (`run::run`, `At::Served`) from the place
//! and the command line the client gives. It never ends by itself: `llx serve stop` ends it.
//! `llx serve status` says what it is.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::builder::{FalseyValueParser, TypedValueParser};
use clap::parser::ValueSource;
use clap::{CommandFactory, FromArgMatches};
use protocol::{Event, Place, ToServer};

use crate::cli::{Cli, Command, RunOptions, ServeAction};
use crate::commands::run::{self, At, Front, Served};
use crate::daemon;
use crate::desk::Request;
use crate::error::{Failure, print};
use crate::exit;
use crate::interrupt;
use crate::server::Ui;
use crate::server::host::{Host, Kept};
use crate::server::serve;
use crate::server::socket::{self, Connection};

/// How long the daemon waits for its lanes to end once stopped; and for another daemon's lock
/// that is being let go (one that was just asked to end).
const ENDING: Duration = Duration::from_secs(2);

pub(crate) fn serve(
    action: Option<&ServeAction>,
    front: &'static dyn Front,
) -> Result<i32, Failure> {
    let config = daemon::config().map_err(Failure::Environment)?;
    let path = daemon::socket_path(&config);
    match action {
        None => host(&config, &daemon::own_socket(&config), front),
        Some(ServeAction::Status) => match front.status(&path, ENDING) {
            Some(found) => {
                let protocol::Status {
                    build,
                    pid,
                    attached,
                    lanes,
                    working,
                    kept,
                    jobs,
                } = found;
                print(&format!(
                    "llx serve: running, pid {pid}, build {build}, {attached} attached, \
                     {lanes} sessions open, {working} working, {kept} in the background, \
                     {jobs} jobs\n"
                ))?;
                Ok(exit::OK)
            }
            None => print("llx serve: not running\n").map(|()| exit::FAILED),
        },
        Some(ServeAction::Stop) => match front.stop(&path) {
            true => print("llx serve: stopped\n").map(|()| exit::OK),
            false => Err(Failure::Environment("the daemon did not stop".into())),
        },
    }
}

/// The daemon itself, until it is asked to end; its sessions shown as `front` shows them.
fn host(config: &Path, path: &Path, front: &'static dyn Front) -> Result<i32, Failure> {
    let _lock = locked(config)?.ok_or_else(|| {
        Failure::Environment(format!(
            "another daemon serves {} (llx serve status)",
            config.display()
        ))
    })?;
    let failed = |error: std::io::Error| Failure::Environment(format!("llx serve: {error}"));
    let listener = socket::bind(path).map_err(failed)?;
    let host = Host::new(daemon::build());
    let pid = std::process::id();
    print(&format!("{} (pid {pid})\n", daemon::READY_LINE))?;
    log::info!("daemon {} serving {}", host.build, path.display());
    daemon::quiet();
    interrupt::install();
    crate::providers::refresh_catalog();
    let served = Arc::clone(&host);
    let each = move |connection| attach(connection, &served, front);
    if socket::accept(listener, each).is_err() {
        return Ok(exit::ENVIRONMENT);
    }
    host.wait();
    log::info!("daemon ending: {} tasks working", interrupt::working());
    // Every task stops and is saved.
    let _ = interrupt::PROCESS.stop();
    let until = Instant::now() + ENDING;
    while interrupt::working() > 0 && Instant::now() < until {
        backoff();
    }
    let _ = std::fs::remove_file(path);
    Ok(exit::OK)
}

/// The lock, waiting a little for a daemon that is ending to let it go.
fn locked(config: &Path) -> Result<Option<daemon::Lock>, Failure> {
    let until = Instant::now() + ENDING;
    loop {
        let lock = daemon::lock(config).map_err(|error| Failure::Environment(error.to_string()))?;
        if lock.is_some() || Instant::now() >= until {
            return Ok(lock);
        }
        backoff();
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "a backoff: neither a lock let go nor a task's end has an event to wait on here"
)]
fn backoff() {
    std::thread::sleep(Duration::from_millis(50));
}

/// One client: back to the sessions moved to the background where it is, else its server and,
/// once it says where it is, its desk until the desk ends. A client that only asks (`status`,
/// `stop`) says nothing of where it is, and gets no desk.
fn attach(connection: Connection, host: &Arc<Host>, front: &'static dyn Front) {
    let Connection { incoming, out } = connection;
    let Ok(first) = incoming.recv() else {
        return;
    };
    if let Some(kept) = rejoinable(&first, host) {
        log::info!(
            "client back to the sessions in the background at {}",
            kept.cwd
        );
        return rejoin(first, &incoming, out, kept, host);
    }
    // Where the client is, as its handshake says: the work it moves to the background is kept
    // there. An interface is attached while its connection lasts; its desk may outlive it, kept.
    let at = placed(&first).map(|(place, _)| place.cwd.clone());
    let attached = at.is_some().then(|| host.attach());
    let daemon = Arc::clone(host);
    let ended: run::Left = Box::new(move |kept, peer, desk| {
        drop(attached);
        match (kept, at) {
            (true, Some(cwd)) => daemon.keep(Kept { cwd, peer, desk }),
            _ => {
                let _ = desk.send(Request::Gone);
            }
        }
    });
    let daemon = Some(Arc::clone(host));
    let served = run::serving(Some(first), incoming, out, daemon, ended);
    let Ok((peer, requests)) = served else {
        return;
    };
    // Nothing the desk is asked counts before the handshake has said where the client is.
    let place = loop {
        match requests.recv() {
            Ok(Request::Attach(place)) => break place,
            Ok(
                Request::Open(_)
                | Request::List
                | Request::Quit
                | Request::Background
                | Request::Gone,
            ) => {}
            Err(_gone) => return,
        }
    };
    log::info!(
        "client attached at {}: llx {}",
        place.cwd,
        place.args.join(" ")
    );
    let _forgotten = Forget(Arc::clone(host), Arc::clone(&peer));
    let ui = Ui::new(peer);
    let told = ui.clone();
    let served = Served {
        place,
        ui,
        requests,
        front,
    };
    if let Err(why) = session(served) {
        log::warn!("client's session failed: {why}");
        told.send(Event::Error(format!("llx: {why}")));
        told.send(Event::Quit);
    }
}

/// The place and build an interface's handshake gives.
fn placed(first: &ToServer) -> Option<(&Place, &str)> {
    match first {
        ToServer::Call {
            command:
                protocol::Command::Initialize {
                    place: Some(place),
                    build,
                    ..
                },
            ..
        } => Some((place, build)),
        ToServer::Call { .. } | ToServer::Reply { .. } => None,
    }
}

/// The sessions moved to the background where `first`'s client is, of its build, when its
/// command line asks for nothing but a session: a task, `resume`, a model or any other option is
/// a new desk's, built from it, and the sessions kept wait for a plain `llx` there. They keep the
/// place they were opened in (its environment, its colours): their work already runs there.
fn rejoinable(first: &ToServer, host: &Host) -> Option<Kept> {
    let (place, build) = placed(first)?;
    let plain = place.args.is_empty() && build == host.build;
    plain.then(|| host.take(&place.cwd)).flatten()
}

/// The client back to `kept`: given what its sessions did meanwhile, then served; kept again if
/// it moves them to the background once more, else their work stops.
fn rejoin(
    first: ToServer,
    incoming: &std::sync::mpsc::Receiver<ToServer>,
    out: std::sync::mpsc::Sender<protocol::ToClient>,
    kept: Kept,
    host: &Arc<Host>,
) {
    let _attached = host.attach();
    let info = run::info(Some(Arc::clone(host)));
    match serve::rejoin(first, incoming, &kept.peer, out, &kept.desk, &info) {
        true => host.keep(kept),
        false => {
            let _ = kept.desk.send(Request::Gone);
        }
    }
}

/// A desk's end, however it ends: nothing of it is kept for a later client.
struct Forget(Arc<Host>, Arc<crate::server::Peer>);

impl Drop for Forget {
    fn drop(&mut self) {
        self.0.forget(&self.1);
    }
}

/// The session the client's command line asks for, where it is.
fn session(served: Served) -> Result<i32, Failure> {
    let cli = command_line(&served.place)?;
    crate::commands::dispatch(cli, At::Served(served))
}

/// How an option the environment may give (`env =` in `cli.rs`) takes its variable's value
/// (`None`: unset or empty, the option's default).
type Set = fn(&mut RunOptions, Option<&str>);

/// The session's options a variable names, each by its argument's id: clap read them from the
/// daemon's environment, and they are the client's.
const FROM_ENV: [(&str, &str, Set); 4] = [
    ("LLX_PROVIDER", "provider", |run, value| {
        run.models.provider = value.map(str::to_string);
    }),
    ("LLX_MODEL", "model", |run, value| {
        run.models.model = value.map(str::to_string);
    }),
    ("LLX_REASONING", "reasoning", |run, value| {
        run.models.reasoning = value.map(str::to_string);
    }),
    ("LLX_NO_ADVICE", "no_advice", |run, value| {
        // As clap reads the variable: `0`, `false`, `no`, `off` and the like are false.
        let falsey = FalseyValueParser::new();
        let parse = |value| falsey.parse_ref(&clap::Command::new("llx"), None, OsStr::new(value));
        run.no_advice = value.is_some_and(|value| parse(value).unwrap_or(true));
    }),
];

/// The client's command line, as it reads where the client is: what it does not say itself, the
/// client's environment does.
fn command_line(place: &Place) -> Result<Cli, Failure> {
    let words = std::iter::once("llx".to_string()).chain(place.args.iter().cloned());
    let usage = |error: clap::Error| Failure::Usage(error.to_string());
    let matches = Cli::command().try_get_matches_from(words).map_err(usage)?;
    let mut cli = Cli::from_arg_matches(&matches).map_err(usage)?;
    if cli.print || !matches!(cli.command, None | Some(Command::Resume(_))) {
        return Err(Failure::Usage("the daemon serves sessions only".into()));
    }
    // The options are those of the innermost command, as `Cli::options` takes them.
    let mut said = &matches;
    while let Some((_, inner)) = said.subcommand() {
        said = inner;
    }
    // Its paths are the client's: the directory it works in, the pictures it attached.
    let options = cli.options();
    options.cd = Some(PathBuf::from(&place.cwd));
    for (var, id, set) in FROM_ENV {
        if said.value_source(id) != Some(ValueSource::CommandLine) {
            let value = place
                .env
                .iter()
                .find(|(name, value)| name == var && !value.is_empty());
            set(options, value.map(|(_, value)| value.as_str()));
        }
    }
    for image in &mut options.images {
        *image = Path::new(&place.dir).join(&*image);
    }
    Ok(cli)
}

#[cfg(test)]
#[path = "serve_tests.rs"]
mod tests;
