//! A command that runs on: started, its output read as it comes, waited for by whoever holds it.
//! `bash` waits for it a while; one still running then goes to the session's jobs, which wait for
//! its end (B9).

use std::io;
use std::path::PathBuf;
use std::process::Child;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::leaders::Tracked;
use super::output::{Output, Span};
use super::process::{self, DRAIN_GRACE, Ending, Finished, Seen};
use super::sandbox::Held;

/// A command started by `bash`, until its end.
#[derive(Debug)]
pub struct Running {
    child: Child,
    output: Arc<Mutex<Output>>,
    reader_ended: Receiver<()>,
    started: Instant,
    /// Whether its output closed: an exit closes it, unless a background process holds it.
    closed: bool,
    /// Held to its end: its scratch directory, and its place among the sessions a quit kills.
    _kept: (Held, Tracked),
}

impl Running {
    pub(super) fn new(
        child: Child,
        output: Arc<Mutex<Output>>,
        reader_ended: Receiver<()>,
        started: Instant,
        kept: (Held, Tracked),
    ) -> Self {
        Self {
            child,
            output,
            reader_ended,
            started,
            closed: false,
            _kept: kept,
        }
    }

    /// How it ended, waited for until `deadline` (killed when `stop` says so); `None` while it
    /// still runs then, left as it is.
    pub(super) fn until(
        &mut self,
        deadline: Instant,
        stop: &dyn Fn() -> bool,
    ) -> io::Result<Option<Finished>> {
        let bounds = (deadline, stop);
        let seen = process::wait(
            &mut self.child,
            bounds,
            self.started,
            &self.reader_ended,
            &mut self.closed,
        )?;
        Ok(match seen {
            Seen::Ended(ending, elapsed) => Some(self.finished(ending, elapsed)),
            Seen::Late => None,
        })
    }

    /// How it ended, killed at `deadline` or when `stop` says so.
    pub(super) fn run_out(
        &mut self,
        deadline: Instant,
        stop: &dyn Fn() -> bool,
    ) -> io::Result<Finished> {
        if let Some(finished) = self.until(deadline, stop)? {
            return Ok(finished);
        }
        let elapsed = self.started.elapsed();
        let killed = process::kill(&mut self.child);
        Ok(self.finished(Ending::TimedOut { killed }, elapsed))
    }

    /// Its end, waited for until `deadline` (killed there, or when `stop` says so), reported as
    /// `bash` reports a command.
    pub fn report(&mut self, deadline: Instant, stop: &dyn Fn() -> bool) -> String {
        match self.run_out(deadline, stop) {
            Ok(finished) => super::bash::report(finished, None),
            Err(error) => {
                let killed = process::kill(&mut self.child);
                format!("Failed: the command could not be waited for ({error}); killed: {killed}")
            }
        }
    }

    /// How long it has run.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// What it printed so far, as the model reads it.
    pub(super) fn so_far(&self) -> String {
        crate::lock(&self.output).so_far()
    }

    /// What it prints, read while whoever holds it waits: for the person to look at.
    #[must_use]
    pub fn watch(&self) -> Watch {
        Watch(Arc::clone(&self.output))
    }

    /// The file every line it prints from now on, and every one it printed, is written to; `None`
    /// when no file could be made.
    pub(super) fn keep_whole(&self) -> Option<PathBuf> {
        crate::lock(&self.output).keep_whole()
    }

    fn finished(&mut self, ending: Ending, elapsed: Duration) -> Finished {
        let open = !self.closed && self.reader_ended.recv_timeout(DRAIN_GRACE).is_err();
        let span = match ending {
            Ending::Exit(0) => Span::Wide,
            Ending::Exit(_)
            | Ending::Signal(_)
            | Ending::TimedOut { .. }
            | Ending::Stopped { .. } => Span::Narrow,
        };
        let captured = crate::lock(&self.output).finish(span);
        Finished {
            ending,
            elapsed,
            output: captured.text,
            cut_lines: captured.cut.then_some(captured.lines),
            open,
        }
    }
}

/// What a [`Running`] command printed, read from another thread while it runs.
#[derive(Debug, Clone)]
pub struct Watch(Arc<Mutex<Output>>);

impl Watch {
    /// What it printed so far, as the model reads it: its start and its end, the middle of a
    /// long output cut.
    #[must_use]
    pub fn so_far(&self) -> String {
        crate::lock(&self.0).so_far()
    }
}

impl Drop for Running {
    /// A command let go before its end is killed with all it started (R10).
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            process::kill(&mut self.child);
        }
    }
}

#[cfg(test)]
#[path = "running_tests.rs"]
mod tests;
