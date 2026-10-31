//! The server's wall clock: Unix seconds, as sessions, tasks and schedules keep time.

use std::time::{SystemTime, UNIX_EPOCH};

/// Unix seconds now; a clock set before the epoch reads as the epoch.
pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
