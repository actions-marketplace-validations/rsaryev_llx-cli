//! Locks and waits shared by the server's threads: a mutex that survives a panicked holder, the
//! bell a thread sleeps on until something may have changed, and work spread over a few threads.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
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

/// `work` done on each of `items` by at most `threads` threads named `name-N`, each taking the
/// next item in turn; the results in the items' order. A panic in `work` is not a `None`: it
/// comes back here once every thread has ended, as `thread::scope` gives it.
pub(crate) fn each_at_once<T: Sync, R: Send>(
    items: &[T],
    threads: usize,
    name: &str,
    work: impl Fn(&T) -> R + Sync,
) -> std::io::Result<Vec<Option<R>>> {
    let slots: Vec<Mutex<Option<R>>> = items.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let take = || {
        loop {
            let at = next.fetch_add(1, Ordering::Relaxed);
            let (Some(item), Some(slot)) = (items.get(at), slots.get(at)) else {
                break;
            };
            let done = work(item);
            *lock(slot) = Some(done);
        }
    };
    thread::scope(|scope| {
        (0..threads.clamp(1, items.len().max(1)))
            .map(|n| {
                let take = &take;
                thread::Builder::new()
                    .name(format!("{name}-{n}"))
                    .spawn_scoped(scope, take)
                    .map(drop)
            })
            .collect::<std::io::Result<Vec<()>>>()
    })?;
    Ok(slots
        .into_iter()
        .map(|slot| slot.into_inner().unwrap_or_else(PoisonError::into_inner))
        .collect())
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
