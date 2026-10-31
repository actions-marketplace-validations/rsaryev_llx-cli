//! The frames: every message is one of these, the same in process and over a socket (W1, W4).
//! A frame's `method` names it as `resource/verb` (W2).

use serde::{Deserialize, Serialize};

use crate::{Asked, Command, Done, Event, Reply};

/// From the server to an interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum ToClient {
    /// What happened in `session` (a lane or an agent), or in the server when `None`. `seq`
    /// numbers what is kept for a client that attaches later (W6); a live-only event has none.
    #[serde(rename = "session/event")]
    Event {
        session: Option<String>,
        event: Event,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
    },
    /// What follows is not all that happened: the ring let some of it go, so what the client
    /// shows may be incomplete, and it starts again from what follows (W6).
    #[serde(rename = "session/gap")]
    Gap,
    /// A question for the person, from `session`'s task; answered by [`ToServer::Reply`].
    #[serde(rename = "person/ask")]
    Ask {
        id: u64,
        session: String,
        asked: Asked,
    },
    /// The question no longer waits: answered elsewhere, or its task stopped.
    #[serde(rename = "person/resolved")]
    Resolved { id: u64 },
    /// What came of the command `id`.
    #[serde(rename = "command/done")]
    Done { id: u64, done: Done },
}

/// From an interface to the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum ToServer {
    /// A command, answered by [`ToClient::Done`] with the same id.
    #[serde(rename = "command/call")]
    Call { id: u64, command: Command },
    /// The person's reply to the question `id`.
    #[serde(rename = "person/reply")]
    Reply { id: u64, reply: Reply },
}

#[cfg(test)]
#[path = "frame_tests.rs"]
mod tests;
