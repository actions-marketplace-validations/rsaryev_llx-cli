//! How the desk's lanes end: each leaves the table as it ends, however it ends; a panic ends the
//! desk; the desk's own end stops every lane; and quitting while lanes work stops them all and
//! waits a bounded time before the process ends anyway.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::Duration;

use crate::server::Ui;
use crate::sync::lock;
use protocol::Event;

/// How long "Exit and stop tasks" waits for the lanes it stopped.
const QUIT_WAIT: Duration = Duration::from_secs(2);

/// The lanes open in every desk of the process, by session id: what a daemon says of itself, and
/// a session never open at two terminals at once (stops are by id, and the newest would win).
static OPEN: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub(crate) fn open_anywhere() -> usize {
    lock(&OPEN).len()
}

/// The lanes open, by session id, oldest first.
#[derive(Default)]
pub(super) struct Lanes {
    live: Mutex<Vec<String>>,
    ended: Condvar,
    /// A lane panicked: the others are stopped.
    pub(super) panicked: AtomicBool,
}

impl Lanes {
    pub(super) fn has(&self, id: &str) -> bool {
        lock(&self.live).iter().any(|live| live == id)
    }

    pub(super) fn count(&self) -> usize {
        lock(&self.live).len()
    }

    /// Open lane `id` here; false when it is open in another desk.
    pub(super) fn join(&self, id: &str) -> bool {
        let mut open = lock(&OPEN);
        if open.iter().any(|known| known == id) {
            return false;
        }
        open.push(id.to_string());
        lock(&self.live).push(id.to_string());
        true
    }

    /// Wait up to `timeout` until `done` holds of the number of lanes open; whether it does.
    pub(super) fn wait_until(&self, timeout: Duration, done: impl Fn(usize) -> bool) -> bool {
        let live = lock(&self.live);
        let (live, _) = self
            .ended
            .wait_timeout_while(live, timeout, |live| !done(live.len()))
            .unwrap_or_else(PoisonError::into_inner);
        done(live.len())
    }

    /// Stop every lane's task: the ids copied out first, as a stop runs its lane's callback and
    /// rings the questions' bell (R6).
    pub(super) fn stop_all(&self) {
        let live = lock(&self.live).clone();
        for id in &live {
            crate::interrupt::stop(id);
        }
    }

    /// "Exit and stop tasks": every lane is stopped; those that do not end within [`QUIT_WAIT`]
    /// are left to `quit`, which ends the process.
    pub(super) fn quit(&self, quit: &dyn Fn()) {
        self.stop_all();
        if !self.wait_until(QUIT_WAIT, |open| open == 0) {
            quit();
        }
    }
}

/// A lane's end, however it ends: it leaves the desk. A panic ends the desk too (and with it the
/// interface), so the terminal the panic hook gave back is not left under an interface drawing.
pub(super) struct Leaving<'a> {
    pub lanes: &'a Lanes,
    pub id: String,
    /// The desk, woken to look whether lanes are left.
    pub wake: std::sync::mpsc::Sender<super::Wake>,
}

impl Drop for Leaving<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.lanes.panicked.store(true, Ordering::SeqCst);
        }
        lock(&self.lanes.live).retain(|live| *live != self.id);
        lock(&OPEN).retain(|open| *open != self.id);
        self.lanes.ended.notify_all();
        let _ = self.wake.send(super::Wake::Left);
    }
}

/// Stops every lane when the desk ends, however it ends, and ends the interface: a lane at its
/// prompt then gets nothing, so the scope waits only for lanes that are ending.
pub(super) struct AllLanes<'a> {
    pub lanes: &'a Lanes,
    pub ui: &'a Ui,
}

impl Drop for AllLanes<'_> {
    fn drop(&mut self) {
        self.lanes.stop_all();
        self.ui.send(Event::Quit);
    }
}
