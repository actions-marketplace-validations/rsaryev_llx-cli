//! The server's side of the seam (ARCHITECTURE.md K2, W1-W8): the sessions' port to the
//! interface, the lanes' input queues, where a client is, what is done with what an interface sends, and how the
//! server says what it knows. Everything here goes, with the sessions it serves, to
//! `apps/server`.

pub(crate) mod asking;
pub(crate) mod host;
pub(crate) mod hub;
pub(crate) mod inputs;
pub(crate) mod label;
pub(crate) mod place;
pub(crate) mod serve;
pub(crate) mod socket;
pub(crate) mod ui;
pub(crate) mod wire;
pub(crate) mod work;

pub(crate) use self::ui::{Peer, Ui};
