//! The server's side of the seam (ARCHITECTURE.md K2, W1-W8): the sessions' port to the
//! interface (`ui`), the lanes' input queues (`inputs`), what is done with what the interface
//! sends (`serve`), the gate's and the person's answers (`asking`), how a call reads (`label`),
//! domain values as the protocol carries them (`wire`), and what runs in the lanes (`work`).

pub(crate) mod asking;
pub(crate) mod inputs;
pub(crate) mod label;
pub(crate) mod serve;
pub(crate) mod ui;
pub(crate) mod wire;
pub(crate) mod work;

pub(crate) use self::ui::{Peer, Ui};
