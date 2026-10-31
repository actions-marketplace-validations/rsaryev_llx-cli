//! The shell tool: [`BashTool`] (`bash`) runs a command in the workspace, after asking the gate,
//! and reports its exit code and output; a command the gate confines runs in the system's sandbox
//! ([`Scope`]).

mod bash;
mod inherit;
mod leaders;
mod output;
mod process;
mod query;
mod sandbox;
mod sweep;

pub use bash::BashTool;
pub use inherit::streams_only;
pub use leaders::{Tracked, kill_all, track};
pub use process::{Quiet, SHELL, kill, scoped, shell};
pub use query::{Unanswered, query};
pub use sandbox::{Held, Scope, permit};
