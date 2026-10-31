//! What is typed at a session's prompt: the slash commands and the help.

use protocol::{LESS_REASONING, MORE_REASONING, SHIFT_TAB};

pub(crate) const HELP: &str = "Type a task and press Enter; each one goes on from the last.
  /model            choose the model and its reasoning level, of those signed in to (also /models)
  /login            sign in to a provider, then choose its model and reasoning level
  /mode [name]      the modes, or switch to one (also /permissions)
  /rewind           the conversation back to before a task; files stay (also /undo, /checkpoint)
  /diff             the changes since the last commit, new files too
  /init             write an AGENTS.md for this project
  /copy             copy the last answer
  /usage            what the session spent (also /cost)
  /new              open a new session; this one goes on in the background (also /clear)
  /status           the model, the workspace, the mode, the session
  /help             this help (also /?)
  /exit             quit (also /quit, or Ctrl-D)
Keys
  Shift+Enter       a new line (also Ctrl-J, or a line ending in \\)
  Up, Down          earlier entries
  Tab               complete a command or an @file; keep a message for after the task
  Ctrl-R            the last earlier entry with the words typed before it; again for older
  Ctrl-G            write the entry in $EDITOR
  Ctrl-U            clear the line before the cursor
  Shift+Left, Right select (with Alt by word, Shift+Home/End to the line's ends)
  Ctrl-Z            suspend llx: `fg` in the shell comes back to it
  Shift+Tab         the next mode (after the task if working)
  Shift+Up, Down    more or less reasoning, as far as the model takes it
  Left              on an empty prompt: the list of sessions, or back to main from an agent
  Down              on an empty prompt: into the list under it (Enter or Right opens, x stops)
  Esc               stop the task (a message typed meanwhile waits for it), or back to main
  Esc Esc           on an empty prompt: /rewind
  Ctrl-C            stop the task, clear the line, or twice to quit\n";

/// One entry at the prompt.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Entry {
    Task(String),
    Login,
    /// `/model`: a model of a provider signed in to, and its reasoning level.
    Model,
    New,
    Status,
    Help,
    Exit,
    /// `/mode`: the modes, the current one marked.
    Modes,
    /// `/mode <name>`, a mode's name or not.
    Switch(String),
    /// Shift+Tab.
    CycleMode,
    /// Shift+Up or Shift+Down: the next level the model takes, up or down.
    Reasoning(bool),
    /// `/rewind` (also `/undo`): back to before one of the conversation's tasks.
    Rewind,
    Diff,
    Copy,
    Usage,
    /// A `/command` that is not one.
    Unknown(String),
}

/// What `/` offers in the interface: each command and what it does.
pub(crate) fn commands() -> Vec<(String, String)> {
    [
        ("help", "the commands and keys"),
        ("model", "choose the model and its reasoning level"),
        (
            "login",
            "sign in to a provider, then choose its model and reasoning",
        ),
        ("mode", "show the modes, or switch: /mode plan"),
        (
            "rewind",
            "the conversation back to before a task; the files stay",
        ),
        ("diff", "the changes since the last commit, new files too"),
        ("init", "write an AGENTS.md for this project"),
        ("copy", "copy the last answer"),
        ("usage", "what the session spent"),
        ("new", "open a new session; this one goes on"),
        ("status", "the model, the workspace, the mode, the session"),
        ("exit", "quit"),
    ]
    .into_iter()
    .map(|(name, what)| (name.to_string(), what.to_string()))
    .collect()
}

/// What `text` asks for; `None` for a blank line.
pub(crate) fn entry(text: &str) -> Option<Entry> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if text == MORE_REASONING || text == LESS_REASONING {
        return Some(Entry::Reasoning(text == MORE_REASONING));
    }
    if text == SHIFT_TAB {
        return Some(Entry::CycleMode);
    }
    let Some(command) = text.strip_prefix('/') else {
        return Some(Entry::Task(text.to_string()));
    };
    let command = command.trim();
    let (name, rest) = command
        .split_once(char::is_whitespace)
        .map_or((command, ""), |(name, rest)| (name, rest.trim()));
    Some(match (name, rest) {
        ("mode" | "permissions", "") => Entry::Modes,
        ("mode" | "permissions", name) => Entry::Switch(name.to_string()),
        (name, "") if protocol::EXIT.contains(&name) => Entry::Exit,
        ("login", "") => Entry::Login,
        ("model" | "models", "") => Entry::Model,
        ("rewind" | "undo" | "checkpoint", "") => Entry::Rewind,
        ("diff", "") => Entry::Diff,
        ("copy", "") => Entry::Copy,
        ("usage" | "cost", "") => Entry::Usage,
        ("init", "") => Entry::Task(super::extras::INIT.into()),
        (name, "") if protocol::NEW.contains(&name) => Entry::New,
        ("status", "") => Entry::Status,
        ("help" | "?", "") => Entry::Help,
        _ => Entry::Unknown(command.to_string()),
    })
}

#[cfg(test)]
#[path = "entry_tests.rs"]
mod tests;
