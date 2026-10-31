//! A short command the app runs for itself (`/diff`'s `git`, the shell `doctor` checks), with a
//! deadline (R10): what it printed on stdout once it exited well; past the deadline it is killed,
//! with everything it started, as a `bash` command is. Its stderr is dropped, and nothing of what
//! it prints is rendered or cut for a model.

use std::fmt;
use std::io;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::process::{self, Ending};

/// What is kept of what a query prints at most: an answer, not a stream.
const MOST: usize = 64 << 20;

/// Why a query gave no answer.
#[derive(Debug)]
pub enum Unanswered {
    /// It could not be started.
    Start(io::Error),
    /// It could not be waited for; it was killed.
    Lost(io::Error),
    /// It ended, not well: how, as a person reads it.
    Ended(String),
    /// Its deadline passed: it was killed, with everything it started.
    Late(Duration),
}

impl fmt::Display for Unanswered {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start(error) => write!(f, "did not start: {error}"),
            Self::Lost(error) => write!(f, "could not be waited for: {error}"),
            Self::Ended(how) => f.write_str(how),
            Self::Late(wait) => write!(f, "did not end within {} s", wait.as_secs()),
        }
    }
}

/// What `command` printed on stdout, once it exited with 0 within `wait`. `command` is as
/// [`super::shell`] makes it, the leader of a session of its own, so the kill at the deadline
/// reaches everything it started.
pub fn query(mut command: Command, wait: Duration) -> Result<Vec<u8>, Unanswered> {
    let (reader, writer) = io::pipe().map_err(Unanswered::Start)?;
    let mut child = (command.stdin(Stdio::null()).stdout(writer))
        .stderr(Stdio::null())
        .spawn()
        .map_err(Unanswered::Start)?;
    // The command holds the pipe's writing end now: ours goes, so its end is the output's end.
    drop(command);
    // Listed until it is reaped, for a quit that cannot wait for it (`leaders`).
    let _listed = super::leaders::track(&child);
    let started = Instant::now();
    let printed = Arc::new(Mutex::new(vec![]));
    let shared = Arc::clone(&printed);
    // Kept as it comes, [`MOST`] of it: what an exit left to a background process is what was
    // read. The rest is read and dropped: a pipe closed under the command would kill it.
    let reader_ended = process::pump(reader, "llx-query", move |bytes| {
        let mut kept = crate::lock(&shared);
        let room = MOST.saturating_sub(kept.len());
        kept.extend(bytes.get(..bytes.len().min(room)).unwrap_or_default());
    });
    // Nothing stops a query but its deadline: it is short, and its caller waits for it.
    let never = || false;
    let bounds = (started + wait, &never as &dyn Fn() -> bool);
    let (ending, _, closed) =
        process::watch(&mut child, bounds, started, &reader_ended).map_err(Unanswered::Lost)?;
    let how = match ending {
        Ending::Exit(0) => {
            // What a background process it started still holds is not waited for past a grace.
            if !closed {
                let _ = reader_ended.recv_timeout(process::DRAIN_GRACE);
            }
            return Ok(std::mem::take(&mut *crate::lock(&printed)));
        }
        Ending::Exit(code) => format!("exited {code}"),
        Ending::Signal(signal) => format!("was killed by signal {signal}"),
        Ending::TimedOut { .. } => return Err(Unanswered::Late(wait)),
        Ending::Stopped { .. } => "was stopped".into(),
    };
    Err(Unanswered::Ended(how))
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;
