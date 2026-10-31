//! The catalog a process keeps: read once, and read again when the file a refresh writes is
//! newer than the copy read, looked at no more often than `every`. A daemon lives for weeks; the
//! catalog it read at its start must not be the one it uses at their end.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Instant, SystemTime};

use models::Catalog;

/// The catalog kept, and when it was last looked at.
pub(super) struct Kept {
    shelf: Mutex<Option<Shelf>>,
    every: std::time::Duration,
}

struct Shelf {
    catalog: Arc<Catalog>,
    /// When the file it was read from was written; `None` when there was none.
    written: Option<SystemTime>,
    looked: Instant,
}

impl Kept {
    pub(super) const fn new(every: std::time::Duration) -> Self {
        Self {
            shelf: Mutex::new(None),
            every,
        }
    }

    /// The catalog: `load` read the first time, and again once `every` has passed and `written`
    /// says the file changed. Whether it is time to ask for a newer one: at each look but the
    /// first (a process asks as it starts).
    pub(super) fn get(
        &self,
        written: impl Fn() -> Option<SystemTime>,
        load: impl Fn() -> Catalog,
    ) -> (Arc<Catalog>, bool) {
        let mut shelf = self.lock();
        let had = match shelf.as_mut() {
            Some(kept) if kept.looked.elapsed() < self.every => {
                return (Arc::clone(&kept.catalog), false);
            }
            Some(kept) => {
                kept.looked = Instant::now();
                Some((Arc::clone(&kept.catalog), kept.written))
            }
            None => None,
        };
        // The file is read with the shelf let go (R6): two threads may both read it, and either
        // copy is the same.
        drop(shelf);
        let now = written();
        if let Some((catalog, _)) = had.as_ref().filter(|(_, at)| *at == now) {
            return (Arc::clone(catalog), true);
        }
        let catalog = Arc::new(load());
        *self.lock() = Some(Shelf {
            catalog: Arc::clone(&catalog),
            written: now,
            looked: Instant::now(),
        });
        (catalog, had.is_some())
    }

    fn lock(&self) -> MutexGuard<'_, Option<Shelf>> {
        crate::sync::lock(&self.shelf)
    }
}

#[cfg(test)]
#[path = "kept_tests.rs"]
mod tests;
