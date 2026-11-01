//! The instruction files the agent follows, found under the names repositories already use for
//! coding agents:
//!
//! - the person's own, for every project: `~/.llx/AGENTS.md`;
//! - the project's: in each directory from the repository's root down to the working directory,
//!   the first of `AGENTS.override.md`, `AGENTS.md`, `CLAUDE.md` (a deeper file comes later and
//!   wins);
//! - a directory below the working directory may hold its own: the prompt says to read it before
//!   working there, and the history keeps what such a read got back ([`reads_one`]).
//!
//! A line that is only `@path` brings in that file, as in CLAUDE.md. Everything is read once
//! and only ever appended, so the conversation's prefix, and the prompt cache, stay as they were.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use contracts::chat::ToolCall;
use serde_json::Value;

/// The names an instruction file has, the first found in a directory being the one read.
const NAMES: [&str; 3] = ["AGENTS.override.md", "AGENTS.md", "CLAUDE.md"];
/// Bytes of one instruction file: a huge one must not crowd out the rest.
pub(crate) const FILE_LIMIT: usize = 16 * 1024;
/// Bytes of all instruction files at the start together.
pub(crate) const TOTAL_LIMIT: usize = 32 * 1024;
/// How deep `@path` imports go.
const IMPORT_DEPTH: usize = 3;
/// The tool the model reads files with, and the bytes its report adds around what it read (the
/// exit code, the time, the directory).
const SHELL: &str = "bash";
const REPORT: usize = 1024;

/// One file's instructions, ready to read.
#[derive(Debug, PartialEq)]
pub(crate) struct Found {
    pub path: PathBuf,
    pub text: String,
}

/// Whether `call` read an instruction file and `output` holds no more than one: a command that
/// names one, as the prompt asks the model to read a deeper directory's. Its rules stay in force,
/// so the history keeps what it got back while it can, where it would elide any other output.
pub(crate) fn reads_one(call: &ToolCall, output: &str) -> bool {
    let command = call.arguments.get("command").and_then(Value::as_str);
    call.name == SHELL
        && output.len() <= FILE_LIMIT + REPORT
        && command.is_some_and(|command| NAMES.iter().any(|name| command.contains(name)))
}

/// The person's own instructions, for every project.
pub(crate) fn personal(home: Option<&Path>) -> Option<Found> {
    let path = home?.join(".llx").join("AGENTS.md");
    read(&path, FILE_LIMIT)
}

/// The project's instructions for `root`: one file per directory from the repository's root
/// down, within [`TOTAL_LIMIT`] bytes all together.
pub(crate) fn project(root: &Path) -> Vec<Found> {
    let mut left = TOTAL_LIMIT;
    let mut out = vec![];
    for dir in chain(root) {
        if left == 0 {
            break;
        }
        let limit = FILE_LIMIT.min(left);
        if let Some(found) = in_dir(&dir, limit) {
            // The note that a file was cut does not count against what is left.
            left -= found.text.len().min(limit);
            out.push(found);
        }
    }
    out
}

/// The directories whose instructions apply to `root`: from the repository's root (the nearest
/// ancestor with `.git`) down to `root`; only `root` outside a repository.
fn chain(root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![];
    for dir in root.ancestors() {
        dirs.push(dir.to_path_buf());
        if dir.join(".git").exists() {
            dirs.reverse();
            return dirs;
        }
    }
    vec![root.to_path_buf()]
}

/// The instruction file of `dir`, if it has one.
fn in_dir(dir: &Path, limit: usize) -> Option<Found> {
    NAMES.iter().find_map(|name| read(&dir.join(name), limit))
}

fn read(path: &Path, limit: usize) -> Option<Found> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut seen = HashSet::from([same(path)]);
    let text = imported(&text, path.parent()?, &mut seen, IMPORT_DEPTH);
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let kept = crate::context::cut(text, limit);
    let more = match text.len() - kept.len() {
        0 => String::new(),
        cut => format!("\n[{cut} more bytes not shown: read the file for the rest]"),
    };
    Some(Found {
        path: path.to_path_buf(),
        text: format!("{kept}{more}"),
    })
}

/// `text` with each line that is only `@path` replaced by that file (relative to `dir`, or
/// `~/…`), to `depth` levels, each file once; a line naming no readable file stays as it is.
fn imported(text: &str, dir: &Path, seen: &mut HashSet<PathBuf>, depth: usize) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let target = line
            .trim()
            .strip_prefix('@')
            .filter(|t| !t.is_empty() && !t.contains(char::is_whitespace));
        let path = target.map(|target| match target.strip_prefix("~/") {
            Some(rest) => std::env::var_os("HOME")
                .map_or_else(|| dir.join(target), |h| PathBuf::from(h).join(rest)),
            None => dir.join(target),
        });
        match path {
            Some(path) if depth > 0 => {
                // A file already brought in: its line goes.
                let key = same(&path);
                if seen.contains(&key) {
                    continue;
                }
                match std::fs::read_to_string(&path) {
                    Ok(inner) => {
                        seen.insert(key);
                        let inner_dir = path.parent().unwrap_or(dir).to_path_buf();
                        out.push_str(imported(&inner, &inner_dir, seen, depth - 1).trim_end());
                    }
                    Err(_) => out.push_str(line),
                }
            }
            // Past the depth: its line goes.
            Some(_) => continue,
            None => out.push_str(line),
        }
        out.push('\n');
    }
    out
}

/// `path` as one file is named however it is reached (`docs/../CLAUDE.md` is `CLAUDE.md`).
fn same(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// A file's instructions as the agent reads them.
pub(crate) fn block(tag: &str, found: &Found) -> String {
    format!(
        "<{tag} path=\"{}\">\n{}\n</{tag}>",
        found.path.display(),
        found.text
    )
}

#[cfg(test)]
#[path = "instructions_tests.rs"]
mod tests;
