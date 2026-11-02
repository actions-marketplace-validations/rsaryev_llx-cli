//! The shell tool: [`BashTool`] (`bash`) runs a command in the workspace, after asking the gate,
//! and reports its exit code and output; a command the gate confines runs in the system's sandbox
//! ([`Scope`]). What the app runs itself starts the same way: a monitor's command ([`permit`],
//! [`scoped`], [`kill`]), a short query ([`query()`]); every session
//! still running is listed ([`track`]), for [`kill_all`] when the process must quit at once.

mod background;
mod bash;
mod inherit;
mod leaders;
mod output;
mod process;
mod query;
mod running;
mod sandbox;
mod sweep;

pub use background::{Adopted, Background};
pub use bash::BashTool;
pub use leaders::{Tracked, kill_all, track};
pub use process::{Quiet, SHELL, kill, scoped, shell};
pub use query::{Unanswered, query};
pub use running::{Running, Watch};
pub use sandbox::{Held, Scope, permit};
