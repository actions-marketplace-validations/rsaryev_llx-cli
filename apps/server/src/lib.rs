//! llx's server: the composition root. It parses the command line and runs its command; it holds
//! the sessions, their lanes, agents and jobs, the modes and the gate's and the person's answers,
//! and serves them through the protocol to the interface (ARCHITECTURE.md D7). The binary hands it
//! the terminal's side (`commands::run::Front`) and ends the process with the code it returns.

mod advice;
mod agents;
mod arrivals;
mod attach;
mod brake;
mod checkpoints;
mod choice;
mod cli;
mod clock;
pub mod commands;
mod compose;
mod context;
mod desk;
pub mod error;
mod exit;
mod gate;
mod input;
mod instructions;
pub mod interrupt;
mod jobs;
mod live;
mod logs;
mod meta;
mod mode;
mod modes;
pub mod output;
mod picker;
mod printable;
mod providers;
mod rules;
mod server;
mod shape;
mod spend;
mod sync;
#[cfg(any(test, feature = "testing"))]
pub mod test_support;
#[cfg(test)]
mod testing;
