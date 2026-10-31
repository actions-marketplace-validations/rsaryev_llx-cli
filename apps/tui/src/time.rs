//! The interface's clocks: wall time as the wire gives it, the time between two instants, and
//! the short window a first Ctrl-C or Esc waits for its second.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// How long a first Ctrl-C at the prompt waits for a second.
const QUIT_WINDOW_MS: u128 = 2000;

/// Now, in milliseconds since the epoch, as the wire gives times.
pub(super) fn now_ms() -> u64 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH);
    u64::try_from(now.unwrap_or_default().as_millis()).unwrap_or(u64::MAX)
}

/// Whether `at`, a first Ctrl-C or Esc, is still within the window a second one counts in.
pub(super) fn in_quit_window(at: Instant, now: Instant) -> bool {
    now.duration_since(at).as_millis() < QUIT_WINDOW_MS
}

/// Milliseconds from `since` to `now`.
pub(super) fn elapsed_ms(now: Instant, since: Instant) -> u64 {
    u64::try_from(now.duration_since(since).as_millis()).unwrap_or(u64::MAX)
}
