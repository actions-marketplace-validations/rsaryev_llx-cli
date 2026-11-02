//! A job's stop, and who asked for it: the person (the interface's `x`) or the run (the model's
//! `jobs stop`, the session's end). What a job gives back as it ends follows who (B10): the
//! person's stop is read with their next task, the run's is already known to it.

use std::sync::atomic::{AtomicU8, Ordering};

const RUNNING: u8 = 0;
const PERSON: u8 = 1;
const RUN: u8 = 2;

/// Who asked a job to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum By {
    Person,
    /// The model, or the session as it ends.
    Run,
}

/// A job's stop: asked once, by the first who asks.
#[derive(Debug, Default)]
pub(super) struct Stop(AtomicU8);

impl Stop {
    /// Ask it to stop; one asked already keeps who asked first.
    pub(super) fn ask(&self, by: By) {
        let code = match by {
            By::Person => PERSON,
            By::Run => RUN,
        };
        let _ = (self.0).compare_exchange(RUNNING, code, Ordering::SeqCst, Ordering::SeqCst);
    }

    /// Whether it was asked to stop.
    pub(super) fn asked(&self) -> bool {
        self.by().is_some()
    }

    /// Who asked it to stop, if anyone did.
    pub(super) fn by(&self) -> Option<By> {
        match self.0.load(Ordering::SeqCst) {
            PERSON => Some(By::Person),
            RUN => Some(By::Run),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "stop_tests.rs"]
mod tests;
