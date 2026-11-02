//! The interface's client to the server (ARCHITECTURE.md W1, L8): over the channels to the server
//! in this process. A command waits for what is done with it, or is only sent; what the server
//! could not do is shown to the person either way (W1); everything else the server sends goes to
//! the interface as it comes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use protocol::{Command, Done, Event, Reply, ToClient, ToServer};

/// The commands sent and not yet done, each waiting where its answer goes, and whether the server
/// is gone: one lock, so a command sent after the end is told so, never left waiting (R7).
#[derive(Default)]
struct Pending {
    waiting: HashMap<u64, SyncSender<Done>>,
    gone: bool,
}

/// The interface's connection to the server; cheap to clone, one per thread that calls.
#[derive(Clone)]
pub(crate) struct Client {
    to_server: Sender<ToServer>,
    pending: Arc<Mutex<Pending>>,
    next: Arc<AtomicU64>,
}

impl Client {
    /// A client over channels to the server in this process: frames from the server that are not
    /// what a command came to go to `deliver`, in order, until the server is gone; a command the
    /// server could not do goes there too, as the error the person is shown.
    pub(crate) fn in_process(
        to_server: Sender<ToServer>,
        from_server: Receiver<ToClient>,
        deliver: impl Fn(ToClient) + Send + 'static,
    ) -> std::io::Result<Self> {
        let pending = Arc::new(Mutex::default());
        let theirs = Arc::clone(&pending);
        std::thread::Builder::new()
            .name("llx-client".into())
            .spawn(move || {
                for frame in from_server {
                    match frame {
                        ToClient::Done { id, done } => {
                            if let Done::Failed { why } = &done {
                                deliver(ToClient::Event {
                                    session: None,
                                    event: Event::Error(format!("llx: {why}")),
                                });
                            }
                            let reply = lock(&theirs).waiting.remove(&id);
                            if let Some(reply) = reply {
                                let _ = reply.send(done);
                            }
                        }
                        frame @ (ToClient::Event { .. }
                        | ToClient::Ask { .. }
                        | ToClient::Resolved { .. }) => deliver(frame),
                    }
                }
                // The server is gone: what waits for it ends now, and what is sent later too.
                let mut pending = lock(&theirs);
                pending.gone = true;
                pending.waiting.clear();
            })?;
        Ok(Self {
            to_server,
            pending,
            next: Arc::new(AtomicU64::new(1)),
        })
    }

    /// `command`, and what was done with it, within `wait`; `None` when the server is gone, late,
    /// or could not do it (which the person is shown).
    pub(crate) fn call(&self, command: Command, wait: Duration) -> Option<Done> {
        let (reply, done) = sync_channel(1);
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        {
            let mut pending = lock(&self.pending);
            if pending.gone {
                return None;
            }
            pending.waiting.insert(id, reply);
        }
        if self.to_server.send(ToServer::Call { id, command }).is_err() {
            lock(&self.pending).waiting.remove(&id);
            return None;
        }
        let done = done.recv_timeout(wait);
        if done.is_err() {
            lock(&self.pending).waiting.remove(&id);
        }
        done.ok()
            .filter(|done| !matches!(done, Done::Failed { .. }))
    }

    /// `command`, not waiting for what is done with it. A server gone already is said so to the
    /// interface as it ends, so nothing is said here.
    pub(crate) fn send(&self, command: Command) {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let _ = self.to_server.send(ToServer::Call { id, command });
    }

    /// The person's reply to the question `id`.
    pub(crate) fn reply(&self, id: u64, reply: Reply) {
        let _ = self.to_server.send(ToServer::Reply { id, reply });
    }
}

/// The table, even after a thread panicked holding it: each change to it is one insert, removal
/// or flag, never left half made.
fn lock(pending: &Mutex<Pending>) -> MutexGuard<'_, Pending> {
    pending.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
