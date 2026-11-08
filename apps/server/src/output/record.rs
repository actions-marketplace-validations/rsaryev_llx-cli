//! `LLX_RECORD`: a session's run kept whole, for reading it again later (how the history was
//! fitted, what an elision cost). Off unless the variable names a directory; then each event is a
//! JSON line of `<dir>/<session>.jsonl` as it happens: a task's goal, each call and its whole
//! output as the model read it (before any elision changed it), the model's words, each model
//! turn's usage, each elision, the answer; an agent's events in its parent's file, named by the
//! agent. The file is the person's alone (0600). The file is opened for each event, appended to
//! and closed: no lock is held across the write (R6). A record that cannot be written is no
//! failure: the run goes on without it.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// Where records go: `LLX_RECORD`, when it names a directory.
pub(super) fn dir() -> Option<PathBuf> {
    std::env::var_os("LLX_RECORD")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
}

/// Append `event` of `agent`'s (empty: the session's own) to `session`'s record in `dir`.
pub(super) fn write(dir: &Path, session: &str, agent: &str, mut event: Value) {
    if session.is_empty() {
        return;
    }
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    if let Some(fields) = event.as_object_mut() {
        fields.insert("at_ms".into(), json!(at));
        if !agent.is_empty() {
            fields.insert("agent".into(), json!(agent));
        }
    }
    let mut line = event.to_string();
    line.push('\n');
    let _ = append(&dir.join(format!("{session}.jsonl")), line.as_bytes());
}

fn append(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    options.open(path)?.write_all(bytes)
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
