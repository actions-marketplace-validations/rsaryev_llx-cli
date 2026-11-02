//! What the interface and the server say to each other (ARCHITECTURE.md W1-W8). The seam's
//! types only: no logic, no crate. The server tells what happened in a session ([`Event`]) and
//! asks the person ([`Asked`]); the interface calls the server ([`Command`]) and answers
//! ([`Reply`]). Every frame is one of [`ToClient`] or [`ToServer`], a value on a channel.

mod asked;
mod command;
mod event;
mod frame;

pub use asked::{Adjustment, Answer, Approval, Asked, Choice, Group, Question, Reply};
pub use command::{Command, Done, Input, Offer, Running, Target};
pub use event::{Event, JobKind, JobRow, Label, Outcome, Saved, Total, Turn, Update};
pub use frame::{ToClient, ToServer};

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
