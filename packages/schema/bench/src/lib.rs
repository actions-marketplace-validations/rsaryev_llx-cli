//! What the benchmarks share: a counting allocator, a timer that takes the median of many rounds,
//! the reference validator built as ours is (formats asserted), and the corpora's places.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Instant;

use serde_json::Value;

/// The system's allocator, every allocation counted: a check's allocations are exact.
struct Counting;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

// SAFETY: every call is passed to the system's allocator as it came.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        // SAFETY: the caller's layout, as `GlobalAlloc::alloc` takes it.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, at: *mut u8, layout: Layout) {
        // SAFETY: a block this allocator gave, with its layout.
        unsafe { System.dealloc(at, layout) }
    }

    unsafe fn realloc(&self, at: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        // SAFETY: a block this allocator gave, with its layout.
        unsafe { System.realloc(at, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// A measured call: its median time, and how many allocations one call made.
#[derive(Clone, Copy)]
pub struct Timing {
    pub ns: f64,
    pub allocations: u64,
}

/// `f` timed: rounds of about 0.2 ms each, the median of 31, after one call that counts what it
/// allocates. The inputs are fixed, so the counts are the same on every run.
pub fn measure<T>(mut f: impl FnMut() -> T) -> Timing {
    let before = ALLOCATIONS.load(Relaxed);
    black_box(f());
    let allocations = ALLOCATIONS.load(Relaxed) - before;
    let started = Instant::now();
    let mut calls = 0u64;
    while started.elapsed().as_micros() < 200 {
        black_box(f());
        calls += 1;
    }
    let calls = calls.max(1);
    let mut rounds: Vec<f64> = (0..31)
        .map(|_| {
            let round = Instant::now();
            for _ in 0..calls {
                black_box(f());
            }
            round.elapsed().as_nanos() as f64 / calls as f64
        })
        .collect();
    rounds.sort_by(f64::total_cmp);
    Timing {
        ns: rounds[15],
        allocations,
    }
}

/// `f` timed by its best of five rounds of about a millisecond: steadier where many schemas
/// are each timed once, as in the corpus.
pub fn best<T>(mut f: impl FnMut() -> T) -> f64 {
    let started = Instant::now();
    let mut calls = 0u64;
    while started.elapsed().as_micros() < 200 {
        black_box(f());
        calls += 1;
    }
    let calls = (calls * 5).max(1);
    (0..5)
        .map(|_| {
            let round = Instant::now();
            for _ in 0..calls {
                black_box(f());
            }
            round.elapsed().as_nanos() as f64 / calls as f64
        })
        .fold(f64::INFINITY, f64::min)
}

/// The reference: `jsonschema` with `format` asserted, as this package checks it.
pub fn reference(schema: &Value) -> Option<jsonschema::Validator> {
    jsonschema::options()
        .should_validate_formats(true)
        .build(schema)
        .ok()
}

/// Where `fetch.sh` puts the corpora, beside this crate.
pub fn data(path: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join(path)
}

/// A time as a table says it.
pub fn shown(ns: f64) -> String {
    match ns {
        n if n < 1_000.0 => format!("{n:.0} ns"),
        n if n < 1_000_000.0 => format!("{:.2} µs", n / 1e3),
        n => format!("{:.2} ms", n / 1e6),
    }
}
