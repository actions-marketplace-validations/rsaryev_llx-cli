//! The binary's (D7): a session at the terminal as the daemon's client (docs/daemon.md,
//! "Lifecycle"): the daemon found, else started; then the interface over its socket, until the
//! session is over. What fails before the interface draws leaves the terminal as it was, for a
//! session in process.

use std::path::Path;

use super::daemon::reach;
use llx::{daemon, interrupt};

/// The session in the daemon, this terminal its interface, until it is over; why not, when it
/// cannot be.
pub(super) fn run(root: &Path) -> Result<(), String> {
    let config = daemon::config()?;
    let path = reach(&config)?;
    let connect: tui::Connect = Box::new(move |deliver| client::Client::connect(&path, deliver));
    let start = tui::Start {
        connect,
        root,
        build: daemon::build(),
        quit_now: interrupt::quit_now,
    };
    let running =
        tui::start(start).map_err(|error| format!("the daemon did not attach ({error})"))?;
    running.wait();
    Ok(())
}
