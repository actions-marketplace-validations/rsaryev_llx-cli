//! What the suites share: a home of their own and the binary run in it (`home`), a scripted model
//! behind a local server (`model`), and, for the flows at a terminal, a pseudo-terminal read as a
//! screen (`term`).

use std::time::{Duration, Instant};

pub(crate) mod home;
pub(crate) mod model;
pub(crate) mod term;

/// What `found` gives once it gives something, looked at again every `every` for `timeout`, and
/// once more at its end; `None` if it never did.
#[expect(
    clippy::disallowed_methods,
    reason = "what a test waits for (a screen, a file, an exit) has no event to wait on: it is looked at again after a pause, for a bounded time"
)]
pub(crate) fn eventually<T>(
    timeout: Duration,
    every: Duration,
    mut found: impl FnMut() -> Option<T>,
) -> Option<T> {
    let end = Instant::now() + timeout;
    loop {
        if let Some(value) = found() {
            return Some(value);
        }
        if Instant::now() > end {
            return None;
        }
        std::thread::sleep(every);
    }
}
