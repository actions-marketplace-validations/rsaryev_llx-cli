//! What is known of a session beside its tasks, and changes while it runs: its title now. A
//! change is an [`Update`]: it reaches the interface as `Event::Updated`, and the session's file
//! through the lane's [`Kept`], the session's one writer (ARCHITECTURE.md K8), which appends each
//! task and each change as it comes, so neither the lane nor the title's thread writes over the
//! other. Whatever makes an update (the title's request) runs on a thread of its own: a task never
//! waits on it.

pub(crate) mod title;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use contracts::history::{HistoryError, Record, Session, Turn};

/// A change to a session's facts; a field left `None` stays as it is.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Update {
    pub title: Option<String>,
}

/// A lane's session as it is written: its writer, shared with the threads that make its updates.
#[derive(Clone)]
pub(crate) struct Kept(Arc<Shared>);

struct Shared {
    record: Box<dyn Record>,
    /// A title was made: none is asked for again.
    titled: AtomicBool,
    /// A title is being asked for.
    naming: AtomicBool,
}

impl Kept {
    pub(crate) fn new(record: Box<dyn Record>) -> Self {
        Self(Arc::new(Shared {
            record,
            titled: AtomicBool::new(false),
            naming: AtomicBool::new(false),
        }))
    }

    /// `turn`, a task that ended, recorded in `session` and written.
    pub(crate) fn turn(&self, session: &mut Session, turn: Turn) -> Result<(), HistoryError> {
        let written = self.0.record.turn(&turn);
        session.record(turn);
        written
    }

    /// `update` written; one the store cannot write is logged, and the interface still shows it.
    pub(crate) fn post(&self, update: &Update) {
        let Some(title) = &update.title else { return };
        self.0.titled.store(true, Ordering::SeqCst);
        if let Err(error) = self.0.record.title(title) {
            log::warn!("the session's title was not saved: {error}");
        }
    }

    /// Whether `session` should be named now: it has no title, none is made yet, and none is
    /// being asked for. While the [`Naming`] returned lives, no other request starts; a request
    /// that fails lets the next task ask again.
    pub(crate) fn begin(&self, session: &Session) -> Option<Naming> {
        let titled = !session.title.is_empty() || self.0.titled.load(Ordering::SeqCst);
        let free = !titled && !self.0.naming.swap(true, Ordering::SeqCst);
        free.then(|| Naming(Arc::clone(&self.0)))
    }
}

/// A title being asked for: when it ends, another may be.
pub(crate) struct Naming(Arc<Shared>);

impl Drop for Naming {
    fn drop(&mut self) {
        self.0.naming.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
#[path = "meta_tests.rs"]
mod tests;
