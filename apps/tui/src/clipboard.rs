//! The person's clipboard, the interface's own (K2): `/copy`'s text is written where the person
//! is, by the system's tool, whichever server sent it.

use std::process::Command;
use std::time::Duration;

use super::within::exits_within;

/// How long the system's tool may take to take the text and exit (R10).
const WAIT: Duration = Duration::from_secs(3);

/// The system's tools, each with its arguments, tried in order.
#[cfg(target_os = "macos")]
const TOOLS: &[&[&str]] = &[&["pbcopy"]];
#[cfg(not(target_os = "macos"))]
const TOOLS: &[&[&str]] = &[
    &["wl-copy"],
    &["xclip", "-selection", "clipboard"],
    &["xsel", "-ib"],
];

/// Write `text` to the system clipboard; what came of it, for the person.
pub(super) fn copy(text: &str) -> String {
    copy_with(text, TOOLS)
}

/// `text` given to the first of `tools` that takes it.
fn copy_with(text: &str, tools: &[&[&str]]) -> String {
    for (name, args) in tools.iter().filter_map(|tool| tool.split_first()) {
        let mut command = Command::new(name);
        command.args(args);
        if exits_within(command, text.as_bytes().to_vec(), WAIT) {
            let count = text.chars().count();
            return format!("Copied the last answer ({count} characters).\n");
        }
    }
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool.first().copied())
        .collect();
    format!("No clipboard tool here ({}).\n", names.join(", "))
}

#[cfg(test)]
#[path = "clipboard_tests.rs"]
mod tests;
