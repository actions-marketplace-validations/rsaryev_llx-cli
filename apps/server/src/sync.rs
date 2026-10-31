//! Locks and waits shared by the server's threads: a mutex that survives a panicked holder, and
//! the bell a thread sleeps on until something may have changed.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

/// `mutex` locked: a thread that panicked holding it left a value that is still whole.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A count rung with a notification, so a ring between a look and the wait is not missed: read
/// [`Bell::rung`] before looking, look, then [`Bell::wait`] on what was read.
#[derive(Default)]
pub(crate) struct Bell {
    rung: Mutex<u64>,
    rang: Condvar,
}

impl Bell {
    pub(crate) const fn new() -> Self {
        Self {
            rung: Mutex::new(0),
            rang: Condvar::new(),
        }
    }

    pub(crate) fn ring(&self) {
        let mut rung = lock(&self.rung);
        *rung = rung.wrapping_add(1);
        self.rang.notify_all();
    }

    pub(crate) fn rung(&self) -> u64 {
        *lock(&self.rung)
    }

    /// Until it rings past `seen`, or `most` passes (forever for `None`).
    pub(crate) fn wait(&self, seen: u64, most: Option<Duration>) {
        let rung = lock(&self.rung);
        let quiet = |now: &mut u64| *now == seen;
        match most {
            Some(most) => drop(self.rang.wait_timeout_while(rung, most, quiet)),
            None => drop(self.rang.wait_while(rung, quiet)),
        }
    }
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
