//! What the interface and the server say to each other (ARCHITECTURE.md W1-W8). Wire types only:
//! no logic, no workspace crate. The server tells what happened in a session ([`Event`]) and asks
//! the person ([`Asked`]); the interface calls the server ([`Command`]) and answers ([`Reply`]).
//! Every frame is one of [`ToClient`] or [`ToServer`], the same in process and over a socket.

mod asked;
mod command;
mod event;
mod frame;

pub use asked::{Adjustment, Answer, Approval, Asked, Choice, Group, Question, Reply};
pub use command::{Command, Done, Input, Place, Running, Status, Target};
pub use event::{Event, JobRow, Label, Outcome, Saved, Total, Turn, Update};
pub use frame::{ToClient, ToServer};

/// The protocol's version: a breaking change is a new one (W3).
pub const VERSION: u32 = 2;

/// The input that switches to the next mode, as Shift+Tab sends it.
pub const SHIFT_TAB: &str = "\u{1b}[Z";
/// The input that raises the reasoning level, as Shift+Up sends it.
pub const MORE_REASONING: &str = "\u{1b}[1;2A";
/// The input that lowers the reasoning level, as Shift+Down sends it.
pub const LESS_REASONING: &str = "\u{1b}[1;2B";
/// The commands that quit the interface: it does them itself.
pub const EXIT: [&str; 2] = ["exit", "quit"];
/// The commands that open a session beside the others: the interface does them itself.
pub const NEW: [&str; 2] = ["new", "clear"];
