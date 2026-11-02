//! The interface's side of the seam (ARCHITECTURE.md W1): one [`Client`] over the channels to
//! the server in this process. A command waits for what is done with it; everything else the
//! server sends goes to the interface as it comes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use protocol::{Command, Done, Reply, ToClient, ToServer};

/// Why a command came to nothing: the server gone or late, or what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    /// The server is gone.
    Gone,
    /// No answer in time.
    Timeout,
    /// The server could not do it, and why.
    Failed(String),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gone => f.write_str("the server is gone"),
            Self::Timeout => f.write_str("the server did not answer in time"),
            Self::Failed(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for CallError {}

/// The commands sent and not yet done, each waiting where its answer goes, and whether the server
/// is gone: one lock, so a command sent after the end is told so, never left waiting (R7).
#[derive(Default)]
struct Pending {
    waiting: HashMap<u64, SyncSender<Done>>,
    gone: bool,
}

/// The interface's connection to the server; cheap to clone, one per thread that calls.
#[derive(Clone)]
pub struct Client {
    to_server: Sender<ToServer>,
    pending: Arc<Mutex<Pending>>,
    next: Arc<AtomicU64>,
}

impl Client {
    /// A client over channels to a server in this process: frames from the server that are not
    /// what a command came to go to `deliver`, in order, until the server is gone.
    pub fn in_process(
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

    /// `command`, and what was done with it, within `wait`.
    pub fn call(&self, command: Command, wait: Duration) -> Result<Done, CallError> {
        let (reply, done) = sync_channel(1);
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        {
            let mut pending = lock(&self.pending);
            if pending.gone {
                return Err(CallError::Gone);
            }
            pending.waiting.insert(id, reply);
        }
        if self.to_server.send(ToServer::Call { id, command }).is_err() {
            lock(&self.pending).waiting.remove(&id);
            return Err(CallError::Gone);
        }
        match done.recv_timeout(wait) {
            Ok(Done::Failed { why }) => Err(CallError::Failed(why)),
            Ok(done) => Ok(done),
            Err(RecvTimeoutError::Timeout) => {
                lock(&self.pending).waiting.remove(&id);
                Err(CallError::Timeout)
            }
            Err(RecvTimeoutError::Disconnected) => Err(CallError::Gone),
        }
    }

    /// `command`, not waiting for what is done with it.
    pub fn send(&self, command: Command) -> Result<(), CallError> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.frame(ToServer::Call { id, command })
    }

    /// The person's reply to the question `id`.
    pub fn reply(&self, id: u64, reply: Reply) -> Result<(), CallError> {
        self.frame(ToServer::Reply { id, reply })
    }

    fn frame(&self, frame: ToServer) -> Result<(), CallError> {
        if lock(&self.pending).gone {
            return Err(CallError::Gone);
        }
        (self.to_server.send(frame)).map_err(|_gone| CallError::Gone)
    }
}

/// The table, even after a thread panicked holding it: each change to it is one insert, removal
/// or flag, never left half made.
fn lock(pending: &Mutex<Pending>) -> MutexGuard<'_, Pending> {
    pending.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
