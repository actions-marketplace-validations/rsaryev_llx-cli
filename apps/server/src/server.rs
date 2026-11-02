//! The server's side of the seam (ARCHITECTURE.md K2, W1-W8): the sessions' port to the
//! interface (`ui`), the lanes' input queues (`inputs`), every client attached and what it was
//! told (`hub`), what is done with what an interface sends (`serve`) and the socket it may come
//! over (`socket`), the daemon as a client sees it (`host`), where a client is (`place`), the
//! gate's and the person's answers (`asking`), how a call reads (`label`), domain values as their
//! wire form (`wire`), and what runs in the lanes (`work`).

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
