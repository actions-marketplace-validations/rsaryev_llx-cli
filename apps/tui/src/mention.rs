//! `@` in the composer: the workspace's files, found by the letters typed after it, so a file is
//! named by its path and the agent reads it. The list is read once, when `@` is first typed, on a
//! thread of its own: the interface draws on meanwhile, and the files show when it arrives.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use super::within::within;

/// Files listed at most: a larger tree is cut, not walked to its end.
const MOST: usize = 20_000;
/// Files offered at once.
const SHOWN: usize = 8;
/// How long git may take to list them (R10): past it, it is killed and the tree walked instead.
const GIT_WAIT: Duration = Duration::from_secs(3);

/// Where the list goes once read: to the interface's thread, as an input that wakes it.
pub(crate) type Arrive = Box<dyn FnOnce(Vec<String>) + Send>;

#[derive(Default)]
pub(crate) struct Files {
    root: Option<PathBuf>,
    /// Taken when the list is first wanted.
    arrive: Option<Arrive>,
    /// Each path read, with its lower case: made once, not at every key.
    list: Vec<(String, String)>,
}

impl Files {
    pub(crate) fn new(root: PathBuf, arrive: Arrive) -> Self {
        Self {
            root: Some(root),
            arrive: Some(arrive),
            list: vec![],
        }
    }

    /// The project the files are in.
    pub(crate) fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Starts reading the list the first time it is wanted, off the interface's thread; it comes
    /// back through the [`Arrive`] given, to be [`Files::loaded`].
    pub(crate) fn load(&mut self) {
        let (Some(root), Some(arrive)) = (self.root.clone(), self.arrive.take()) else {
            return;
        };
        let reader = thread::Builder::new().name("llx-files".into());
        // No thread to read it: no files are offered.
        let _ = reader.spawn(move || arrive(list(&root)));
    }

    /// The list, read.
    pub(crate) fn loaded(&mut self, list: Vec<String>) {
        let lowered = list.into_iter().map(|path| (path.to_lowercase(), path));
        self.list = lowered.collect();
    }

    /// The files whose paths hold `query`'s letters in order, the closest first.
    pub(crate) fn matching(&self, query: &str) -> Vec<String> {
        let query = query.to_lowercase();
        let found = (self.list.iter())
            .filter_map(|(lower, path)| score(lower, &query).map(|s| (s, path.len(), path)));
        // Only the closest are shown: each match goes in among them, in order, or not at all.
        let mut closest = Vec::with_capacity(SHOWN + 1);
        for one in found {
            let at = closest.partition_point(|kept| *kept < one);
            if at < SHOWN {
                closest.insert(at, one);
                closest.truncate(SHOWN);
            }
        }
        closest
            .into_iter()
            .map(|(_, _, path)| path.clone())
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn with(list: Vec<String>) -> Self {
        let mut files = Self::default();
        files.loaded(list);
        files
    }
}

/// How well a path (`lower`, in lower case) matches `query` (lower is better): the file's name
/// holding it whole is best, then the path holding it whole, then its letters in order; `None`
/// when they are not there.
fn score(lower: &str, query: &str) -> Option<usize> {
    let name = lower.rsplit('/').next().unwrap_or(lower);
    if query.is_empty() {
        return Some(3);
    }
    if name.starts_with(query) {
        return Some(0);
    }
    if name.contains(query) {
        return Some(1);
    }
    if lower.contains(query) {
        return Some(2);
    }
    let mut letters = lower.chars();
    query.chars().all(|q| letters.any(|c| c == q)).then_some(4)
}

/// The workspace's files: what git tracks and would track, else a walk that skips hidden
/// folders and build output.
fn list(root: &Path) -> Vec<String> {
    let mut git = Command::new("git");
    git.args(["ls-files", "-co", "--exclude-standard"])
        .current_dir(root);
    if let Some(out) = within(git, GIT_WAIT) {
        let out = String::from_utf8_lossy(&out);
        return out.lines().take(MOST).map(str::to_string).collect();
    }
    let mut out = vec![];
    walk(root, root, &mut out);
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= MOST {
            return;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        // The entry's own type, as git lists it: a linked folder is one entry, never walked
        // into (a link to `.` would be walked again at every level).
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            walk(root, &path, out);
        } else if let Ok(relative) = path.strip_prefix(root) {
            out.push(relative.to_string_lossy().to_string());
        }
    }
}

#[cfg(test)]
#[path = "mention_tests.rs"]
mod tests;
