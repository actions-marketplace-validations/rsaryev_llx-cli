//! llx as a person uses it: the real binary in a pseudo-terminal, keys pressed and text typed,
//! the screen read back, a scripted model in place of the network (`common::model`,
//! `common::term`). Each test is one flow end to end; a regression a person hit gets a flow here.

use std::time::Duration;

mod agents;
mod approval;
mod commands;
mod daemon;
mod desks;
mod keys;
mod picker;
mod quit;
mod rewind;
mod sessions;
mod steering;
mod stop;
mod suspend;

/// How long a flow waits for what the screen should show.
const LONG: Duration = Duration::from_secs(20);
