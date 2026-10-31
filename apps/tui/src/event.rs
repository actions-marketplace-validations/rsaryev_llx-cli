//! What reaches the interface and what it sends. A session's events are the protocol's
//! (`protocol::Event`); the interface's thread takes them, the questions and the terminal's keys
//! as one [`Input`], and sends commands and replies as [`Out`].

use crossterm::event::KeyEvent;
use protocol::{Command, Reply, ToClient};

/// What the interface's thread takes: a frame from the server, the terminal's, or the
/// workspace's files for `@`, read on a thread of their own.
#[derive(Debug)]
pub(crate) enum Input {
    Frame(ToClient),
    Key(KeyEvent),
    Paste(String),
    Resize(u16, u16),
    Files(Vec<String>),
}

/// What the interface sends the server.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Out {
    Call(Command),
    /// The person's reply to the question with this id.
    Reply(u64, Reply),
}
