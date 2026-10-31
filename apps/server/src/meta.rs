//! What is known of a session beside its tasks, and changes while it runs: its title now, other
//! fields later. A change is an [`Update`]. It reaches the interface as `Event::Updated`, and the
//! saved session through [`Pending`]: the lane applies what is pending before each of its saves,
//! and a session already on disk is written at once, under the same lock, so neither write loses
//! the other's. Whatever makes an update (the title's request) runs on a thread of its own: a
//! task never waits on it.

pub(crate) mod title;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use contracts::history::{History, HistoryError, Session};

/// A change to a session's facts; a field left `None` stays as it is.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Update {
    pub title: Option<String>,
}

impl Update {
    fn apply(&self, session: &mut Session) {
        if let Some(title) = &self.title {
            session.title.clone_from(title);
        }
    }

    /// `newer` over this one, field by field.
    fn merge(&mut self, newer: Update) {
        if newer.title.is_some() {
            self.title = newer.title;
        }
    }
}

/// The updates a lane's session has not saved yet, shared with the threads that make them.
#[derive(Clone, Default)]
pub(crate) struct Pending(Arc<Shared>);

#[derive(Default)]
struct Shared {
    update: Mutex<Update>,
    /// A title is being asked for.
    naming: AtomicBool,
}

// The lock is held across the session file's load and save, against R6's letter and by design:
// the file is written by two threads, the lane (its tasks) and the title's (`post`), each a whole
// read, change and write. R7 wants each of them one critical section: were the lock let go
// between `post`'s load and its save, a task the lane saved meanwhile would be written over by
// the older session `post` loaded. The wait it costs is one small local file, and only the lane
// and the title's thread ever take this lock: nothing an interface or another lane waits on is
// behind it.
impl Pending {
    /// Save `session` with what is pending applied.
    pub(crate) fn save(
        &self,
        history: &dyn History,
        session: &mut Session,
    ) -> Result<(), HistoryError> {
        let pending = self.lock();
        pending.apply(session);
        history.save(session)
    }

    /// Keep `update` for the lane's next save, and write it now into session `id` when it is
    /// saved already.
    pub(crate) fn post(&self, history: &dyn History, id: &str, update: Update) {
        let mut pending = self.lock();
        if let Ok(mut saved) = history.load(Some(id)) {
            update.apply(&mut saved);
            let _ = history.save(&saved);
        }
        pending.merge(update);
    }

    /// Whether `session` should be named now: it has no title, none is made yet, and none is
    /// being asked for. While the [`Naming`] returned lives, no other request starts; a request
    /// that fails lets the next task ask again.
    pub(crate) fn begin(&self, session: &Session) -> Option<Naming> {
        let titled = !session.title.is_empty() || self.lock().title.is_some();
        let free = !titled && !self.0.naming.swap(true, Ordering::SeqCst);
        free.then(|| Naming(Arc::clone(&self.0)))
    }

    fn lock(&self) -> MutexGuard<'_, Update> {
        crate::sync::lock(&self.0.update)
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
