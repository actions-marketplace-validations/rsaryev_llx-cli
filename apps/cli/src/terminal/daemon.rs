//! The binary's (D7): the daemon's client side, a client's way to its daemon (docs/daemon.md,
//! "Lifecycle"): the one that answers, if it is of this build; one of another build stopped when
//! idle (`Status::idle`), kept while in use (this session then runs in process); none, or the one
//! stopped, replaced by `llx serve` started in a session of its own, its ready line awaited. What
//! the daemon is (its socket, its lock, its build, how it is started) is the host's
//! (`llx::daemon`, the server's).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc::{RecvTimeoutError, sync_channel};
use std::time::Duration;

use protocol::{Command, Done};

use llx::daemon::{READY_LINE, build, socket_path};

/// How long a daemon has to say what it is.
const ASK: Duration = Duration::from_secs(2);
/// How long a daemon has to start, and to end when asked.
const READY: Duration = Duration::from_secs(10);
const ENDING: Duration = Duration::from_secs(10);

/// The daemon at `path`, if one answers within `wait`: a socket file with nobody behind it, or a
/// server that does not answer, is none.
pub(super) fn status(path: &Path, wait: Duration) -> Option<protocol::Status> {
    let client = client::Client::connect(path, |_| {}).ok()?;
    match client.call(Command::Status, wait) {
        Ok(Done::Status(status)) => Some(status),
        _ => None,
    }
}

/// The socket of this build's daemon for `config`, started when there is none; why not, when
/// it cannot be had.
pub(super) fn reach(config: &Path) -> Result<PathBuf, String> {
    let path = socket_path(config);
    let ours = build();
    if let Some(found) = status(&path, ASK) {
        if found.build == ours {
            return Ok(path);
        }
        // One still in use is never stopped under a terminal, or under work kept for one.
        if !found.idle() {
            let (pid, other) = (found.pid, found.build);
            log::warn!("the daemon (pid {pid}) is build {other}, not {ours}, and in use");
            return Err(format!(
                "the daemon (pid {pid}) is build {other} and in use"
            ));
        }
        log::info!(
            "replacing the idle daemon (pid {}) of build {}",
            found.pid,
            found.build
        );
        if !stop(&path) {
            return Err(format!("the daemon (pid {}) did not stop", found.pid));
        }
    }
    spawn(config)?;
    log::info!("daemon {ours} started");
    // The daemon that serves may have been started where `$TMPDIR` was another: its lock says.
    Ok(socket_path(config))
}

/// Ask the daemon at `path` to end, and wait until it has: its connection ends with it. Whether
/// none is left.
pub(super) fn stop(path: &Path) -> bool {
    let (gone, ended) = sync_channel::<()>(1);
    // What it sends is nothing to us; the connection's end is all we wait for.
    let Ok(client) = client::Client::connect(path, move |_| {
        let _ = &gone;
    }) else {
        return true;
    };
    // A daemon under load answers late: a stop is given as long as the daemon has to end. One
    // that ends before its answer leaves is gone, which is what was asked.
    match client.call(Command::Shutdown, ENDING) {
        Ok(_) | Err(client::CallError::Gone) => {}
        Err(client::CallError::Timeout | client::CallError::Failed(_)) => return false,
    }
    matches!(
        ended.recv_timeout(ENDING),
        Err(RecvTimeoutError::Disconnected)
    )
}

/// `llx serve` started as the host says it runs (`daemon::command`), its ready line awaited.
fn spawn(config: &Path) -> Result<(), String> {
    let why = |error: std::io::Error| format!("the daemon could not start: {error}");
    let mut command = llx::daemon::command(config).map_err(why)?;
    command.stdout(Stdio::piped());
    let mut child = command.spawn().map_err(why)?;
    let out = child.stdout.take().ok_or("the daemon has no stdout")?;
    let (said, line) = sync_channel(1);
    let reader = std::thread::Builder::new().name("llx-daemon-ready".into());
    reader
        .spawn(move || {
            let mut first = String::new();
            let _ = BufReader::new(out).read_line(&mut first);
            let _ = said.send(first);
        })
        .map_err(why)?;
    match line.recv_timeout(READY) {
        Ok(first) if first.starts_with(READY_LINE) => Ok(()),
        // Another client's daemon took the lock first: that one serves.
        Ok(_) if status(&socket_path(config), ASK).is_some() => Ok(()),
        Ok(_) => {
            let _ = child.wait();
            Err("the daemon did not start: see daemon.log in llx's config directory".into())
        }
        Err(_late) => {
            let _ = child.kill();
            Err("the daemon did not start in time".into())
        }
    }
}

#[cfg(test)]
#[path = "daemon_tests.rs"]
mod tests;
