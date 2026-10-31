//! The daemon as a client sees it (docs/daemon.md): its build, the interfaces attached, the
//! sessions moved to the background waiting for the next, and its end when one asks for it. In
//! process there is none: `serve` refuses what only a daemon does.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, Weak};

use protocol::Done;

use super::Peer;
use crate::desk::Request;
use crate::sync::{Bell, lock};

/// Sessions moved to the background in a directory: their peer, and their desk's requests.
pub(crate) struct Kept {
    pub cwd: String,
    pub peer: Arc<Peer>,
    pub desk: Sender<Request>,
}

pub(crate) struct Host {
    pub(crate) build: String,
    attached: AtomicUsize,
    shelf: Mutex<Shelf>,
    ended: Bell,
}

/// What is kept for the next interface, and the peers whose desk ended, which never are: the
/// two change under one lock, so a desk that ends as its connection does is not kept dead (R7).
#[derive(Default)]
struct Shelf {
    kept: Vec<Kept>,
    /// Let go once nothing else holds them.
    ended: Vec<Weak<Peer>>,
}

/// An interface attached while this lives: held by its connection, not its desk, which may go on
/// in the background once the connection ends.
pub(crate) struct Attached(Arc<Host>);

impl Drop for Attached {
    fn drop(&mut self) {
        self.0.attached.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Host {
    pub(crate) fn new(build: String) -> Arc<Self> {
        Arc::new(Self {
            build,
            attached: AtomicUsize::new(0),
            shelf: Mutex::default(),
            ended: Bell::new(),
        })
    }

    pub(crate) fn attach(self: &Arc<Self>) -> Attached {
        self.attached.fetch_add(1, Ordering::SeqCst);
        Attached(Arc::clone(self))
    }

    /// Sessions moved to the background, for the next interface in their directory, unless
    /// their desk has ended.
    pub(crate) fn keep(&self, kept: Kept) {
        let mut shelf = lock(&self.shelf);
        let peer = Arc::as_ptr(&kept.peer);
        if !shelf.ended.iter().any(|ended| ended.as_ptr() == peer) {
            shelf.kept.push(kept);
        }
    }

    /// The sessions moved to the background in `cwd`, taken for an interface attaching there.
    pub(crate) fn take(&self, cwd: &str) -> Option<Kept> {
        let kept = &mut lock(&self.shelf).kept;
        let at = kept.iter().position(|kept| kept.cwd == cwd)?;
        Some(kept.remove(at))
    }

    /// `peer`'s sessions ended: nothing is kept of them, now or later.
    pub(crate) fn forget(&self, peer: &Arc<Peer>) {
        let mut shelf = lock(&self.shelf);
        shelf.kept.retain(|kept| !Arc::ptr_eq(&kept.peer, peer));
        shelf.ended.retain(|ended| ended.strong_count() > 0);
        shelf.ended.push(Arc::downgrade(peer));
    }

    /// What `llx serve status` says.
    pub(crate) fn status(&self) -> Done {
        Done::Status(protocol::Status {
            build: self.build.clone(),
            pid: std::process::id(),
            attached: self.attached.load(Ordering::SeqCst),
            lanes: crate::desk::open_anywhere(),
            working: crate::interrupt::working(),
            kept: lock(&self.shelf).kept.len(),
            jobs: crate::interrupt::jobs(),
        })
    }

    /// Asked to end: [`Host::wait`] returns.
    pub(crate) fn end(&self) {
        self.ended.ring();
    }

    /// Until the daemon is asked to end.
    pub(crate) fn wait(&self) {
        self.ended.wait(0, None);
    }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
