//! The directory the tools work in: where a command runs, and where the paths the model writes
//! resolve. A file may be read anywhere (a dependency's source, a system header), but changed only
//! inside the root, so a slip in a path never touches the rest of the machine; a directory the app
//! keeps to itself (its keys, its records) is never one.

use std::io;
use std::path::{Component, Path, PathBuf};

use contracts::tool::ToolError;

#[derive(Debug)]
pub struct Workspace {
    /// Canonical, so a path is compared with it after symlinks are resolved.
    root: PathBuf,
    /// Files and directories no tool may read or change, however they are reached (the app's own
    /// key and records): canonical where they exist.
    denied: Vec<PathBuf>,
    /// The environment commands run with, when it is not this process's: the client's.
    env: Option<Vec<(String, String)>>,
}

impl Workspace {
    /// The workspace at `root`, which must exist.
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            root: root.as_ref().canonicalize()?,
            denied: vec![],
            env: None,
        })
    }

    /// The same workspace, with `paths` (and everything under them) off limits to the tools.
    #[must_use]
    pub fn denying(mut self, paths: impl IntoIterator<Item = PathBuf>) -> Self {
        self.denied
            .extend(paths.into_iter().map(|path| real(&path)));
        self
    }

    /// The same workspace, its commands run with `env` instead of this process's environment.
    #[must_use]
    pub fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.env = Some(env);
        self
    }

    pub fn env(&self) -> Option<&[(String, String)]> {
        self.env.as_deref()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// What the app keeps to itself, canonical where it exists: a read-only command's sandbox
    /// leaves it unread where the system can.
    pub(crate) fn denied(&self) -> &[PathBuf] {
        &self.denied
    }

    /// Where `path` points, for reading: relative to the root, or absolute; `~` is the home
    /// directory. `.` and `..` are resolved by name, so the result is clean in reports.
    pub fn readable(&self, path: &str) -> Result<PathBuf, ToolError> {
        let path = path.trim();
        if path.is_empty() {
            return Err(ToolError::Failed("the path is empty".into()));
        }
        let given = home(path).unwrap_or_else(|| PathBuf::from(path));
        let wanted = normal(&self.root.join(given));
        if !self.denied.is_empty() {
            // Where it really is: a symlink to a denied file is denied too.
            let real = real(&wanted);
            if self.denied.iter().any(|denied| real.starts_with(denied)) {
                return Err(ToolError::Failed(format!(
                    "{} is off limits to the tools",
                    wanted.display()
                )));
            }
        }
        Ok(wanted)
    }

    /// Where `path` points, for changing: as [`Self::readable`], but only inside the root once
    /// symlinks are followed. The result is the real location, so a write through a symlink lands
    /// on its target instead of replacing the link.
    pub(crate) fn writable(&self, path: &str) -> Result<PathBuf, ToolError> {
        let wanted = self.readable(path)?;
        let real = real(&wanted);
        if !real.starts_with(&self.root) {
            return Err(ToolError::Failed(format!(
                "{} is outside the workspace {}: files are changed only inside it",
                wanted.display(),
                self.root.display()
            )));
        }
        Ok(real)
    }

    /// `path` as a report shows it: relative to the root when inside it, else as it is.
    pub fn shown(&self, path: &Path) -> String {
        match path.strip_prefix(&self.root) {
            Ok(inside) if inside.as_os_str().is_empty() => ".".into(),
            Ok(inside) => inside.display().to_string(),
            Err(_) => path.display().to_string(),
        }
    }
}

/// `~` or `~/rest` in the home directory, when it is known.
fn home(path: &str) -> Option<PathBuf> {
    let rest = path.strip_prefix('~')?;
    if !(rest.is_empty() || rest.starts_with('/')) {
        return None;
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(rest.trim_start_matches('/')))
}

/// `path` with `.` dropped and each `..` taking off the name before it, without touching the disk.
fn normal(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other @ (Component::Prefix(_) | Component::RootDir | Component::Normal(_)) => {
                out.push(other)
            }
        }
    }
    out
}

/// `path` with its deepest existing ancestor canonicalized: symlinks on the way are followed, and
/// the names that do not exist yet (a file to create) are kept as they are.
pub(crate) fn real(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut missing = vec![];
    loop {
        if let Ok(canonical) = existing.canonicalize() {
            return missing
                .iter()
                .rev()
                .fold(canonical, |acc: PathBuf, name| acc.join(name));
        }
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                missing.push(name.to_os_string());
                existing = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
