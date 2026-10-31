use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;

#[test]
fn a_ring_between_the_look_and_the_wait_is_not_missed() {
    let bell = Bell::new();
    let seen = bell.rung();
    bell.ring();
    // Would sleep forever if the wait took the count as it is now.
    bell.wait(seen, None);
    assert_eq!(bell.rung(), seen + 1);
}

#[test]
fn a_quiet_bell_gives_up_at_the_limit() {
    let bell = Bell::new();
    let began = Instant::now();
    bell.wait(bell.rung(), Some(Duration::from_millis(30)));
    assert!(began.elapsed() >= Duration::from_millis(30));
}

#[test]
fn a_ring_from_another_thread_wakes_a_sleeper() {
    let bell = Arc::new(Bell::new());
    let seen = bell.rung();
    let ringer = Arc::clone(&bell);
    let thread = std::thread::Builder::new()
        .name("ringer".into())
        .spawn(move || ringer.ring())
        .unwrap();
    bell.wait(seen, None);
    thread.join().unwrap();
}

#[test]
fn a_lock_survives_a_panicked_holder() {
    let mutex = Arc::new(Mutex::new(7));
    let held = Arc::clone(&mutex);
    let holder = std::thread::Builder::new().name("holder".into());
    let _ = holder
        .spawn(move || {
            let _guard = held.lock().unwrap();
            panic!("holder");
        })
        .unwrap()
        .join();
    assert_eq!(*lock(&mutex), 7);
}
