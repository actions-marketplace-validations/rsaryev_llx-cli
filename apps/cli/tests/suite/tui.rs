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

/// A command that runs until the file `name` exists in the workspace (at most 10 s): what keeps a
/// task working while a flow acts on it. The screen shows it as `Running for i in`.
fn until_file(name: &str) -> String {
    format!("for i in $(seq 200); do [ -e {name} ] && break; sleep 0.05; done")
}
