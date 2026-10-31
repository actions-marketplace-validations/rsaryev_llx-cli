//! Credential files: `<config>/<app slug>/<name>`, readable by the person alone (0600).
//! What they hold is never printed or logged.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::App;

/// `<config>/<app slug>`: `$XDG_CONFIG_HOME`, else `~/.config`.
pub fn dir(app: App) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join(app.slug))
}

/// `<config>/<app slug>/<name>`.
pub fn path(app: App, name: &str) -> Option<PathBuf> {
    dir(app).map(|dir| dir.join(name))
}

/// The file's text, trimmed; `None` when it is missing or empty. Trimmed in place: a file of
/// megabytes (the model catalog) is not held twice.
pub fn read(path: &Path) -> Option<String> {
    let mut text = std::fs::read_to_string(path).ok()?;
    text.truncate(text.trim_end().len());
    text.drain(..text.len() - text.trim_start().len());
    (!text.is_empty()).then_some(text)
}

/// Write `text` to `path`, readable by the person alone: whole or not at all, through a new file
/// renamed over the old one, so a save cut short (a process ending mid-write) leaves the old one.
pub fn save(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Its own name per save, the whole file name kept: two saves at once, from two processes or
    // two of the daemon's sessions (threads of one process), never write into one file, nor do
    // two files of one stem (`models.json`, `models.etag`).
    static SAVES: AtomicU64 = AtomicU64::new(0);
    let Some(name) = path.file_name() else {
        return Err(std::io::Error::other(format!(
            "{}: not a file",
            path.display()
        )));
    };
    let mut temporary = name.to_os_string();
    let n = SAVES.fetch_add(1, Ordering::Relaxed);
    temporary.push(format!(".tmp-{}-{n}", std::process::id()));
    let temporary = path.with_file_name(temporary);
    let mut options = std::fs::OpenOptions::new();
    // New, so it is created at 0600 whatever a file left behind had.
    options.write(true).create_new(true);
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let written = options
        .open(&temporary)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .and_then(|()| std::fs::rename(&temporary, path));
    if written.is_err() {
        // What is left of it is nobody's; the save's own error is the one to report.
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

/// Remove `path`; whether there was one.
pub fn remove(path: &Path) -> std::io::Result<bool> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// `text` as a key, trimmed; `None` when it cannot be one (empty, or with spaces inside).
pub fn as_key(text: &str) -> Option<&str> {
    let key = text.trim();
    let bad = key.is_empty() || key.contains(|c: char| c.is_whitespace() || c.is_control());
    (!bad).then_some(key)
}

/// `key` as it may be shown: its first six and last four characters, or nothing of a short one.
#[must_use]
pub fn masked(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() < 16 {
        return "...".into();
    }
    let start: String = chars[..6].iter().collect();
    let end: String = chars[chars.len() - 4..].iter().collect();
    format!("{start}...{end}")
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
