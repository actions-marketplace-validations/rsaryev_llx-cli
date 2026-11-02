//! A confined command (ARCHITECTURE.md B3): the operating system keeps it to what its scope lets
//! it change, whatever its text says. It reads every file but the app's own (on macOS); it writes
//! `/dev/null`, a scratch directory of its own (its `TMPDIR`, removed when it ends) and, in the
//! workspace's scope on macOS, the workspace but any `.git` in it (a nested repository's too; on
//! Linux, where none could be kept, nothing more); it opens no network or local socket, so it reaches no other process to act
//! for it. On macOS Seatbelt (`sandbox-exec`, a profile closed by default), on Linux Landlock and
//! a seccomp filter. Where neither can be set up, the command is refused with the reason, never
//! run open.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use contracts::gate::{Effect, Gate, Needs, Verdict};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

/// What a command may do, as the gate answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Whatever the person running llx may.
    Open,
    /// Read, and change the workspace's files alone.
    Workspace,
    /// Read, and change nothing.
    ReadOnly,
}

impl Scope {
    /// What a gate's verdict lets a command do; a refusal is its reason.
    fn of(verdict: Verdict) -> Result<Self, String> {
        match verdict {
            Verdict::Allow => Ok(Self::Open),
            Verdict::Workspace => Ok(Self::Workspace),
            Verdict::ReadOnly => Ok(Self::ReadOnly),
            Verdict::Deny(reason) => Err(reason),
        }
    }
}

/// What a command may do, as `gate` answers `effect`; a refusal is its reason. A workspace verdict
/// where no sandbox can be set up is asked again, as a command that must run as it is (B3, B6).
pub fn permit(gate: &dyn Gate, effect: Effect) -> Result<Scope, String> {
    let scope = Scope::of(gate.check(effect))?;
    if scope != Scope::Workspace || available() {
        return Ok(scope);
    }
    let summary = format!(
        "{}  (no sandbox can be set up here: it would run as it is)",
        effect.summary
    );
    Scope::of(gate.check(Effect {
        summary: &summary,
        needs: Needs::Open,
        ..effect
    }))
}

/// Words a refusal by the sandbox leaves in a command's output, lowercased: the system's own
/// (EPERM, EACCES, EROFS), the sandbox's names, and a name lookup with no network.
const REFUSALS: [&str; 10] = [
    "operation not permitted",
    "permission denied",
    "read-only file system",
    "sandbox",
    "seccomp",
    "landlock",
    "failed to write",
    "could not resolve host",
    "temporary failure in name resolution",
    "nodename nor servname",
];

/// Whether a confined command that exited with `code` looks refused by its sandbox, as Codex
/// guesses it: a code a shell gives a usage error or a missing program (2, 126, 127) is not.
pub(super) fn refused(code: i32, output: &str) -> bool {
    if matches!(code, 0 | 2 | 126 | 127) {
        return false;
    }
    let output = output.to_lowercase();
    REFUSALS.iter().any(|words| output.contains(words))
}

/// What a confined command holds while it runs: its scratch directory, removed when this drops,
/// and on Linux the sandbox's rules, applied in the child before it starts.
#[derive(Debug, Default)]
pub struct Held {
    scratch: Option<PathBuf>,
    #[cfg(target_os = "linux")]
    rules: Option<std::os::fd::OwnedFd>,
}

impl Held {
    /// The command's scratch directory: its `TMPDIR`, set after the environment it runs with, so
    /// a client's own `TMPDIR` does not replace it.
    pub(super) fn scratch(&self) -> Option<&Path> {
        self.scratch.as_deref()
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        if let Some(scratch) = &self.scratch {
            let _ = std::fs::remove_dir_all(scratch);
        }
    }
}

/// Where a workspace command may write besides its scratch: the workspace, unless it is the home
/// directory or holds it (every file the person has). On Linux nowhere: Landlock grants a whole
/// tree and cannot keep a `.git` in it (the workspace's own, a nested repository's, one made
/// anew) from the command, so it only reads.
pub(super) fn writable(root: &Path) -> Option<&Path> {
    if cfg!(target_os = "linux") {
        return None;
    }
    let home = std::env::var_os("HOME").map(|home| crate::workspace::real(Path::new(&home)));
    (!home.is_some_and(|home| home.starts_with(root))).then_some(root)
}

/// Whether this system can confine a command at all.
pub(super) fn available() -> bool {
    #[cfg(target_os = "macos")]
    return macos::available();
    #[cfg(target_os = "linux")]
    return linux::available();
}

/// `sh -c command`, confined: it writes `writes` (and everything under it) besides its scratch,
/// and reads nothing of `kept` where the system can; the command to start (a session's leader, as
/// an open one is) and what it holds until it ends.
#[cfg(target_os = "macos")]
pub(super) fn confined(
    command: &str,
    writes: Option<&Path>,
    kept: &[PathBuf],
) -> io::Result<(Command, Held)> {
    let scratch = scratch()?;
    // The scratch is kept, to be removed, while the command it was made for runs.
    let held = Held {
        scratch: Some(scratch.clone()),
    };
    let shell = macos::shell(command, &scratch, writes, kept)?;
    Ok((shell, held))
}

/// Landlock grants what it lets read and cannot take a path out of it: `kept` is left to the
/// workspace's own check, and nothing is written but the scratch (`writable` gives no place here).
#[cfg(target_os = "linux")]
pub(super) fn confined(
    command: &str,
    _writes: Option<&Path>,
    _kept: &[PathBuf],
) -> io::Result<(Command, Held)> {
    let scratch = scratch()?;
    // As on macOS; the rules' descriptor too, until the child has taken them on.
    let mut held = Held {
        scratch: Some(scratch.clone()),
        rules: None,
    };
    let rules = linux::rules(&scratch)?;
    let shell = linux::shell(command, &rules);
    held.rules = Some(rules);
    Ok((shell, held))
}

/// A new directory of the command's own under the system's temporary one, readable by the person
/// alone, as the sandbox names it: symlinks resolved (macOS's `/var` is `/private/var`).
fn scratch() -> io::Result<PathBuf> {
    static MADE: AtomicU64 = AtomicU64::new(0);
    let name = format!(
        "llx-sandbox-{}-{}",
        std::process::id(),
        MADE.fetch_add(1, Ordering::Relaxed)
    );
    let dir = std::env::temp_dir().join(name);
    let mut builder = std::fs::DirBuilder::new();
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(&dir)?;
    std::fs::canonicalize(&dir)
}

#[cfg(test)]
#[path = "sandbox_tests.rs"]
mod tests;
