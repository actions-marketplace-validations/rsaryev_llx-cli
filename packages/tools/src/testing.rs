//! What the tools' tests share: a fresh directory per test, and a gate that records what it was
//! asked.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use contracts::gate::{Effect, Gate, Needs, Verdict};

/// A directory for one test, removed when the test ends: tests run in parallel, so each gets its
/// own, and no crate is needed for it.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let name = format!(
            "llx-tools-{}-{nanos}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let dir = std::env::temp_dir().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    /// Write `content` at `name` under the directory, creating its parents.
    pub(crate) fn file(&self, name: &str, content: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A descriptor of this process open without close-on-exec, as one is in the moment between its
/// making and its marking (std makes a pipe on macOS, then marks it close-on-exec), at 200 or
/// above, past the descriptors a test process starts with; closed when dropped. With the shell
/// command that says whether it reached the command: `inherited`, else `closed`.
pub(crate) fn stray() -> (std::os::fd::OwnedFd, String) {
    use std::os::fd::FromRawFd;
    // SAFETY: F_DUPFD makes a new descriptor for stderr, without close-on-exec.
    let fd = unsafe { libc::fcntl(2, libc::F_DUPFD, 200) };
    assert!(fd >= 200, "{}", std::io::Error::last_os_error());
    let probe = format!("[ -e /dev/fd/{fd} ] && echo inherited || echo closed");
    // SAFETY: fcntl just made it, and nothing else owns it.
    (unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) }, probe)
}

/// A gate that records each effect as `tool: summary` (` [open]` after one that must leave its
/// sandbox), and gives `answer` to each.
pub(crate) struct Recorder {
    pub asked: Mutex<Vec<String>>,
    pub answer: Verdict,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::answering(Verdict::Allow)
    }
}

impl Recorder {
    pub(crate) fn answering(answer: Verdict) -> Self {
        Self {
            asked: Mutex::default(),
            answer,
        }
    }

    pub(crate) fn denying(reason: &str) -> Self {
        Self::answering(Verdict::Deny(reason.into()))
    }
}

impl Gate for Recorder {
    fn check(&self, effect: Effect) -> Verdict {
        let open = match effect.needs {
            Needs::Sandbox => "",
            Needs::Open => " [open]",
        };
        self.asked
            .lock()
            .unwrap()
            .push(format!("{}: {}{open}", effect.tool, effect.summary));
        self.answer.clone()
    }
}
