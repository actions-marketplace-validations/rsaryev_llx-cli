use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use super::*;

/// A catalog file written at `at` seconds, and how many times it was read.
struct File {
    at: AtomicU64,
    reads: AtomicUsize,
}

impl File {
    fn written(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(self.at.load(Ordering::SeqCst))
    }

    fn load(&self) -> Catalog {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Catalog::built_in()
    }
}

#[test]
fn a_catalog_written_since_it_was_read_is_read_again() {
    let kept = Kept::new(Duration::ZERO);
    let file = File {
        at: AtomicU64::new(1),
        reads: AtomicUsize::new(0),
    };
    let (first, ask) = kept.get(|| Some(file.written()), || file.load());
    assert!(!ask, "a process asks as it starts, not at its first read");
    let (same, ask) = kept.get(|| Some(file.written()), || file.load());
    assert!(ask, "each look after asks for a newer one");
    assert!(
        Arc::ptr_eq(&first, &same),
        "unchanged, it is not read again"
    );
    file.at.store(2, Ordering::SeqCst);
    let (newer, _) = kept.get(|| Some(file.written()), || file.load());
    assert!(!Arc::ptr_eq(&first, &newer), "the refreshed file is read");
    assert_eq!(file.reads.load(Ordering::SeqCst), 2);
}

#[test]
fn the_file_is_looked_at_no_more_often_than_asked() {
    let kept = Kept::new(Duration::from_secs(3600));
    let file = File {
        at: AtomicU64::new(1),
        reads: AtomicUsize::new(0),
    };
    let _ = kept.get(|| Some(file.written()), || file.load());
    file.at.store(2, Ordering::SeqCst);
    let (_, ask) = kept.get(|| Some(file.written()), || file.load());
    assert!(!ask);
    assert_eq!(file.reads.load(Ordering::SeqCst), 1);
}
