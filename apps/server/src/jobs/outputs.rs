//! What each command in the background printed, known by its job's key (`interrupt::job_key`)
//! for as long as its job is listed: the interface asks for it to show the person
//! (`Command::JobOutput`), as the stops of `interrupt.rs` are known for the person's keys.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::sync::lock;

type Read = Arc<dyn Fn() -> String + Send + Sync>;

/// The outputs by key, each with the number its [`Shown`] removes it by.
static OUTPUTS: Mutex<Vec<(String, u64, Read)>> = Mutex::new(Vec::new());
static NEXT: AtomicU64 = AtomicU64::new(0);

/// An output known by its key until this is dropped, as its job leaves the list.
#[must_use = "the output is forgotten when this is dropped"]
pub(crate) struct Shown(u64);

impl Drop for Shown {
    fn drop(&mut self) {
        lock(&OUTPUTS).retain(|(_, number, _)| *number != self.0);
    }
}

/// Know `read` as what the job of `key` printed so far.
pub(crate) fn register(key: &str, read: impl Fn() -> String + Send + Sync + 'static) -> Shown {
    let number = NEXT.fetch_add(1, Ordering::SeqCst);
    lock(&OUTPUTS).push((key.to_string(), number, Arc::new(read)));
    Shown(number)
}

/// What the job of `key` printed so far; `None` when no job of that key is listed.
pub(crate) fn of(key: &str) -> Option<String> {
    let read = (lock(&OUTPUTS).iter().rev())
        .find(|(known, _, _)| known == key)
        .map(|(_, _, read)| Arc::clone(read));
    // Read outside the lock: it takes the command's own.
    read.map(|read| read())
}

#[cfg(test)]
#[path = "outputs_tests.rs"]
mod tests;
