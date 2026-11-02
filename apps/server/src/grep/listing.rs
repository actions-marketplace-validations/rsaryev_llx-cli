//! The files a search reads: under each path given, those git tracks or would (its ignore rules
//! kept), else every file under it but hidden ones and build output; then only text, a size at
//! most, read once.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::Failure;

/// A file the search reads: its path as shown, its text.
pub(super) struct Doc {
    pub(super) path: String,
    pub(super) text: String,
}

/// Past this a file is data or generated, not what a person reads to understand the code.
const MOST_BYTES: u64 = 1 << 20;
/// How many files one search reads at most.
const MOST_FILES: usize = 100_000;
/// How long `git ls-files` may take before the folder is walked instead (R10).
const GIT_WAIT: Duration = Duration::from_secs(10);
/// Directories a walk leaves out where git cannot say what is ignored.
const BUILT: &[&str] = &["target", "node_modules", "__pycache__"];

/// Every file to read under `paths` (`.` when none), each once, in a stable order: each as it
/// is reached from `base`, which a relative path is taken from (the current directory, or the
/// workspace's root).
pub(super) fn listed(base: &Path, paths: &[PathBuf]) -> Result<Vec<String>, Failure> {
    let here = [PathBuf::from(".")];
    let paths = if paths.is_empty() { &here[..] } else { paths };
    let mut files = vec![];
    for path in paths {
        let full = base.join(path);
        let meta = std::fs::metadata(&full)
            .map_err(|error| Failure::Usage(format!("{}: {error}", path.display())))?;
        if meta.is_dir() {
            let prefix = shown(path);
            match tracked(&full, &prefix) {
                Some(found) => files.extend(found),
                None => walk(&full, &prefix, &mut files),
            }
        } else {
            files.push(shown(path));
        }
        if files.len() >= MOST_FILES {
            files.truncate(MOST_FILES);
            break;
        }
    }
    files.retain(|file| !noise(file));
    files.sort_unstable();
    files.dedup();
    Ok(files)
}

/// The text of `path` (as reached from `base`), unless it is too large, unreadable, or not text
/// (a NUL in its start).
pub(super) fn read(base: &Path, path: &str) -> Option<Doc> {
    let file = std::fs::File::open(base.join(path)).ok()?;
    if file.metadata().ok()?.len() > MOST_BYTES {
        return None;
    }
    let mut bytes = vec![];
    file.take(MOST_BYTES).read_to_end(&mut bytes).ok()?;
    if bytes.iter().take(8192).any(|&byte| byte == 0) {
        return None;
    }
    Some(Doc {
        path: path.to_string(),
        text: String::from_utf8_lossy(&bytes).into_owned(),
    })
}

/// What git lists under `dir`, tracked or not yet but not ignored, each under `prefix`; `None`
/// outside a repository.
fn tracked(dir: &Path, prefix: &str) -> Option<Vec<String>> {
    let mut git = tools::shell::shell("git ls-files -z --cached --others --exclude-standard");
    git.current_dir(dir);
    let printed = tools::shell::query(git, GIT_WAIT).ok()?;
    let listed = (printed.split(|&byte| byte == 0))
        .filter(|name| !name.is_empty())
        .map(|name| under(prefix, &String::from_utf8_lossy(name)));
    Some(listed.take(MOST_FILES).collect())
}

/// Every file under `top` but hidden ones and build output, links not followed, each under
/// `prefix`.
fn walk(top: &Path, prefix: &str, files: &mut Vec<String>) {
    let mut pending = vec![top.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if name.starts_with('.') || (kind.is_dir() && BUILT.contains(&name.as_ref())) {
                continue;
            }
            if kind.is_dir() {
                pending.push(entry.path());
            } else if !kind.is_symlink() {
                let path = entry.path();
                let inside = path.strip_prefix(top).unwrap_or(&path);
                files.push(under(prefix, &inside.to_string_lossy()));
                if files.len() >= MOST_FILES {
                    return;
                }
            }
        }
    }
}

/// `name` inside the directory shown as `prefix`.
fn under(prefix: &str, name: &str) -> String {
    match prefix {
        "." | "" => name.to_string(),
        prefix => format!("{}/{name}", prefix.trim_end_matches('/')),
    }
}

/// A path as the person gave it, without a leading `./`.
fn shown(path: &Path) -> String {
    let shown = path.to_string_lossy();
    shown.strip_prefix("./").unwrap_or(&shown).to_string()
}

/// A file no question about code is answered by: a lock file, a minified bundle, a source map.
fn noise(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.ends_with(".lock")
        || name == "package-lock.json"
        || name.ends_with(".min.js")
        || name.ends_with(".map")
}

#[cfg(test)]
#[path = "listing_tests.rs"]
mod tests;
