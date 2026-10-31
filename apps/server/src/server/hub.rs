//! Every interface attached to the sessions, and what they were told (W5, W6). An event goes to
//! every client attached and, unless it is live only, into a bounded ring under its number
//! ([`ring`]), so a client that attaches later gets what the ring keeps (a gap and every
//! session's state first, when its bounds let some go), then the questions still waiting.
//! A client whose channel is closed is dropped; none is waited for (the socket cuts slow ones).
//!
//! Frames leave in the order they were posted, the same for every client, yet no lock is held
//! while one is sent (R6): posting appends to an outbox under the lock, and one thread at a time
//! (whoever finds no one else at it) empties it outside the lock.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};

use protocol::{Event, ToClient};

use self::ring::{Frame, Ring};

mod ring;

/// An attached client, by the hub that knows it.
pub(crate) type ClientId = u64;

/// Events kept for a client that attaches later.
pub(crate) const KEPT: usize = 2_000;

/// Bytes the ring keeps at most, about: long tool reports are replayed only while they fit.
pub(super) const KEPT_BYTES: usize = 8 << 20;

/// Whether `event` is worth giving again to a client that attaches. A step is drawn as a passing
/// line (W6); a copy, a quit, a prefill and a list are what one client asked for once.
/// The rest is replayed: a queue or a total is sent whole, so only the latest is kept.
fn replayable(event: &Event) -> bool {
    !matches!(
        event,
        Event::Step(_) | Event::Copy(_) | Event::Quit | Event::Prefill(_) | Event::Saved(_)
    )
}

struct Client {
    id: ClientId,
    out: Sender<ToClient>,
    /// The last post it has from its replay: what was posted to all before is in it already.
    since: u64,
}

/// A frame posted: its post's number, for whom, and the frame.
type Post = (u64, To, Frame);
/// A client as a flush sees it: its id, its channel, and its last post replayed.
type Seen = (ClientId, Sender<ToClient>, u64);

#[derive(Clone, Copy)]
enum To {
    All,
    One(ClientId),
}

struct State {
    clients: Vec<Client>,
    ring: Ring,
    /// The questions waiting for a reply, in the order asked.
    asked: Vec<(u64, Frame)>,
    /// What is posted and not yet sent, each with its post's number.
    outbox: VecDeque<Post>,
    posted: u64,
    next_client: ClientId,
}

pub(crate) struct Hub {
    state: Mutex<State>,
    sending: AtomicBool,
}

impl Hub {
    /// A hub that keeps the last `room` events, of `bytes` at most.
    pub(crate) fn new(room: usize, bytes: usize) -> Self {
        let state = State {
            clients: vec![],
            ring: Ring::new(room, bytes),
            asked: vec![],
            outbox: VecDeque::new(),
            posted: 0,
            next_client: 0,
        };
        Self {
            state: Mutex::new(state),
            sending: AtomicBool::new(false),
        }
    }

    /// `frame` to every client: an event numbered and kept, a question kept until resolved.
    pub(crate) fn send(&self, frame: ToClient) {
        {
            let mut state = self.lock();
            let frame = state.keep(frame);
            state.post(To::All, frame);
        }
        self.flush();
    }

    /// `frame` to client `id` alone (what came of its command), in order with the rest.
    pub(crate) fn to(&self, id: ClientId, frame: ToClient) {
        self.lock().post(To::One(id), Arc::new(frame));
        self.flush();
    }

    /// Attach `out`: first what the ring keeps, then the questions waiting, then all that
    /// happens. Its id.
    pub(crate) fn attach(&self, out: Sender<ToClient>) -> ClientId {
        let id = {
            let mut state = self.lock();
            let id = state.next_client;
            state.next_client += 1;
            state.replay(id);
            let since = state.posted;
            state.clients.push(Client { id, out, since });
            id
        };
        self.flush();
        id
    }

    /// Client `id` hears nothing more; how many are still attached.
    pub(crate) fn detach(&self, id: ClientId) -> usize {
        let mut state = self.lock();
        state.clients.retain(|client| client.id != id);
        state.clients.len()
    }

    #[cfg(test)]
    pub(crate) fn attached(&self) -> usize {
        self.lock().clients.len()
    }

    /// Send what is posted, unless another thread is at it: it sends ours too. Checked again
    /// after letting go, for what was posted while it finished.
    fn flush(&self) {
        while !self.sending.swap(true, Ordering::AcqRel) {
            while let Some((batch, clients)) = self.take() {
                let gone = send(batch, &clients);
                if !gone.is_empty() {
                    self.lock()
                        .clients
                        .retain(|client| !gone.contains(&client.id));
                }
            }
            self.sending.store(false, Ordering::Release);
            let empty = self.lock().outbox.is_empty();
            if empty {
                return;
            }
        }
    }

    /// What is posted, and who is attached as it was posted.
    fn take(&self) -> Option<(Vec<Post>, Vec<Seen>)> {
        let mut state = self.lock();
        if state.outbox.is_empty() {
            return None;
        }
        let batch = state.outbox.drain(..).collect();
        let clients = (state.clients.iter())
            .map(|client| (client.id, client.out.clone(), client.since))
            .collect();
        Some((batch, clients))
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        crate::sync::lock(&self.state)
    }
}

impl State {
    fn post(&mut self, to: To, frame: Frame) {
        self.posted += 1;
        self.outbox.push_back((self.posted, to, frame));
    }

    /// `frame` as it is sent: an event numbered and kept unless live only; a question kept
    /// while it waits.
    fn keep(&mut self, frame: ToClient) -> Frame {
        match frame {
            ToClient::Event { session, event, .. } if replayable(&event) => {
                self.ring.keep(session, event)
            }
            ToClient::Ask { id, .. } => {
                let frame = Arc::new(frame);
                self.asked.push((id, Arc::clone(&frame)));
                frame
            }
            ToClient::Resolved { id } => {
                self.asked.retain(|(of, _)| *of != id);
                Arc::new(frame)
            }
            frame @ (ToClient::Event { .. } | ToClient::Gap | ToClient::Done { .. }) => {
                Arc::new(frame)
            }
        }
    }

    /// Post to `id` what the ring keeps ([`Ring::replay`]), then the questions waiting.
    fn replay(&mut self, id: ClientId) {
        let mut frames = self.ring.replay();
        frames.extend(self.asked.iter().map(|(_, frame)| Arc::clone(frame)));
        for frame in frames {
            self.post(To::One(id), frame);
        }
    }
}

/// Each frame to whom it is for; the clients whose channel is closed.
fn send(batch: Vec<Post>, clients: &[Seen]) -> Vec<ClientId> {
    let mut gone = Vec::new();
    for (posted, to, frame) in batch {
        for (id, out, since) in clients {
            let wanted = match to {
                To::All => posted > *since,
                To::One(one) => one == *id,
            };
            if wanted && !gone.contains(id) && out.send(ToClient::clone(&frame)).is_err() {
                gone.push(*id);
            }
        }
    }
    gone
}

#[cfg(test)]
#[path = "hub_tests.rs"]
mod tests;
