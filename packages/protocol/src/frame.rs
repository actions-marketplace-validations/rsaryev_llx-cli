//! The frames: every message between the interface and the server is one of these, a value on
//! one of two channels, never encoded (W1).

use crate::{Asked, Command, Done, Event, Reply};

/// From the server to the interface.
#[derive(Debug, Clone, PartialEq)]
pub enum ToClient {
    /// What happened in `session` (a lane or an agent), or in the server when `None`.
    Event {
        session: Option<String>,
        event: Event,
    },
    /// A question for the person, from `session`'s task; answered by [`ToServer::Reply`].
    Ask {
        id: u64,
        session: String,
        asked: Asked,
    },
    /// The question no longer waits: answered, or withdrawn (its task stopped, its lane closed).
    Resolved { id: u64 },
    /// What came of the command `id`.
    Done { id: u64, done: Done },
}

/// From the interface to the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToServer {
    /// A command, answered by [`ToClient::Done`] with the same id.
    Call { id: u64, command: Command },
    /// The person's reply to the question `id`.
    Reply { id: u64, reply: Reply },
}
