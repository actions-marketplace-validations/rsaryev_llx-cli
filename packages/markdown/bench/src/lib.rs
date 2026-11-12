//! What the benchmarks share: an allocator that counts, a timer that takes the median of many
//! rounds, the parsers each behind one name, and the texts.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Instant;

pub mod corpus;
pub mod parsers;

/// The system's allocator, every allocation counted, and the bytes asked for.
struct Counting;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

// SAFETY: every call is passed to the system's allocator as it came.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        BYTES.fetch_add(layout.size() as u64, Relaxed);
        // SAFETY: the caller's layout, as `GlobalAlloc::alloc` takes it.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, at: *mut u8, layout: Layout) {
        // SAFETY: a block this allocator gave, with its layout.
        unsafe { System.dealloc(at, layout) }
    }

    unsafe fn realloc(&self, at: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        BYTES.fetch_add(size.saturating_sub(layout.size()) as u64, Relaxed);
        // SAFETY: a block this allocator gave, with its layout.
        unsafe { System.realloc(at, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// A measured call: its median time, and what one call asked of the allocator. The texts are
/// fixed, so the counts are the same on every run and every machine.
#[derive(Clone, Copy)]
pub struct Timing {
    pub ns: f64,
    pub allocations: u64,
    pub bytes: u64,
}

/// `f` timed alone: see [`measure_together`].
pub fn measure<T>(rounds: usize, mut f: impl FnMut() -> T) -> Timing {
    let mut call = || {
        black_box(f());
    };
    let mut one: [&mut dyn FnMut(); 1] = [&mut call];
    measure_together(rounds, &mut one)[0]
}

/// Each of `fs` timed, a round of about a millisecond of each in turn, so a slow moment of the
/// machine falls on all of them: the median of `rounds`, after one call that counts what it
/// allocates.
pub fn measure_together(rounds: usize, fs: &mut [&mut dyn FnMut()]) -> Vec<Timing> {
    let mut plans = Vec::new();
    for f in fs.iter_mut() {
        let (count, bytes) = (ALLOCATIONS.load(Relaxed), BYTES.load(Relaxed));
        f();
        let allocations = ALLOCATIONS.load(Relaxed) - count;
        let bytes = BYTES.load(Relaxed) - bytes;
        let started = Instant::now();
        let mut calls = 0u64;
        while started.elapsed().as_micros() < 1000 {
            f();
            calls += 1;
        }
        plans.push((calls.max(1), allocations, bytes, Vec::with_capacity(rounds)));
    }
    for _ in 0..rounds {
        for (f, plan) in fs.iter_mut().zip(&mut plans) {
            let round = Instant::now();
            for _ in 0..plan.0 {
                f();
            }
            plan.3
                .push(round.elapsed().as_nanos() as f64 / plan.0 as f64);
        }
    }
    plans
        .into_iter()
        .map(|(_, allocations, bytes, mut times)| {
            times.sort_by(f64::total_cmp);
            Timing {
                ns: times[rounds / 2],
                allocations,
                bytes,
            }
        })
        .collect()
}

/// A time as a person reads it.
pub fn shown(ns: f64) -> String {
    if ns >= 1e9 {
        format!("{:.2} s", ns / 1e9)
    } else if ns >= 1e7 {
        format!("{:.0} ms", ns / 1e6)
    } else if ns >= 1e6 {
        format!("{:.2} ms", ns / 1e6)
    } else if ns >= 1e4 {
        format!("{:.0} µs", ns / 1e3)
    } else {
        format!("{:.2} µs", ns / 1e3)
    }
}

/// A size as a person reads it.
pub fn size(bytes: u64) -> String {
    if bytes >= 10 << 20 {
        format!("{:.0} MB", bytes as f64 / 1048576.0)
    } else if bytes >= 1 << 20 {
        format!("{:.1} MB", bytes as f64 / 1048576.0)
    } else if bytes >= 10 << 10 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    }
}

/// A count with its thousands apart.
pub fn count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// The machine the numbers were taken on, one line.
pub fn machine() -> String {
    let run = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
            .args(args)
            .output()
            .ok()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
            .filter(|text| !text.is_empty())
    };
    let cpu = run("sysctl", &["-n", "machdep.cpu.brand_string"])
        .or_else(|| run("uname", &["-m"]))
        .unwrap_or_default();
    let system = run("uname", &["-sr"]).unwrap_or_default();
    let rustc = run("rustc", &["--version"]).unwrap_or_default();
    format!("{cpu}, {system}, {rustc}")
}
