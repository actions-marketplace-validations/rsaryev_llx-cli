//! The whole of an output the window left something out of, in a file the model can read the rest
//! from (`sed -n`, `rg`) instead of running the command again. Files live in a directory of the
//! user's own under the temporary directory (0700, each file 0600: an output may hold a secret),
//! a day at most, and some hundreds of MiB in all.

use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use super::render::Ends;

/// How long a spill is kept: the model reads one in the turns after the command.
const KEEP_FOR: Duration = Duration::from_secs(24 * 60 * 60);
/// The most a file keeps: past it, its last line says how much more there was.
const MOST: u64 = 64 * 1024 * 1024;
/// The most the directory keeps in all; the oldest files go first.
const MOST_IN_ALL: u64 = 512 * 1024 * 1024;

/// Spills made by this process, for a name no other takes.
static MADE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub(super) struct Spill {
    path: PathBuf,
    file: BufWriter<File>,
    written: u64,
    /// The most this file keeps ([`MOST`]; less in a test).
    most: u64,
    /// Lines not written, from the first that did not fit on.
    left: usize,
    /// Whether it still takes what comes.
    taking: Taking,
    /// The last piece written did not end its line.
    mid_line: bool,
}

/// Whether a spill still takes what comes: once a piece is not written, nothing after it is, so
/// its lines stay numbered as the output's (the cut is read from it by number).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Taking {
    Yes,
    /// A piece did not fit in [`Spill::most`].
    Full,
    /// A write failed: the file holds what came before.
    Broken,
}

impl Spill {
    /// A new spill under `root` (the temporary directory), after the old ones are removed; none
    /// when the directory cannot be made, or is not this user's alone.
    pub(super) fn open(root: &Path) -> Option<Self> {
        let dir = root.join(name());
        if let Err(error) = DirBuilder::new().mode(0o700).create(&dir)
            && error.kind() != std::io::ErrorKind::AlreadyExists
        {
            return None;
        }
        let held = fs::symlink_metadata(&dir).ok()?;
        let private =
            held.is_dir() && held.uid() == uid() && held.permissions().mode() & 0o077 == 0;
        if !private {
            return None;
        }
        prune(&dir);
        let (path, file) = (0..100).find_map(|_| {
            let n = MADE.fetch_add(1, Ordering::Relaxed);
            let path = dir.join(format!("{}-{n}.log", std::process::id()));
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
                .ok()?;
            Some((path, file))
        })?;
        Some(Self {
            path,
            file: BufWriter::new(file),
            written: 0,
            most: MOST,
            left: 0,
            taking: Taking::Yes,
            mid_line: false,
        })
    }

    /// Write `text`, and a line break when it `ends` its line.
    pub(super) fn write(&mut self, text: &str, ends: Ends) {
        let end: &[u8] = match ends {
            Ends::Line => b"\n",
            Ends::Later => b"",
        };
        let bytes = u64::try_from(text.len() + end.len()).unwrap_or(u64::MAX);
        if self.taking == Taking::Yes && self.written + bytes > self.most {
            self.taking = Taking::Full;
        }
        match self.taking {
            Taking::Yes => {}
            Taking::Full | Taking::Broken => {
                self.left += usize::from(ends == Ends::Line);
                return;
            }
        }
        self.written += bytes;
        self.mid_line = ends == Ends::Later;
        let wrote = self
            .file
            .write_all(text.as_bytes())
            .and_then(|()| self.file.write_all(end));
        if wrote.is_err() {
            self.taking = Taking::Broken;
        }
    }

    /// Written to its end; where it is.
    pub(super) fn finish(&mut self) -> PathBuf {
        if self.left > 0 {
            // A line kept only in part is ended first: the note is a line of its own.
            let start = if self.mid_line { "\n" } else { "" };
            let lines = match self.left {
                1 => "1 more line was".to_string(),
                n => format!("{n} more lines were"),
            };
            let note = format!("{start}... {lines} not kept ...\n");
            let _ = self.file.write_all(note.as_bytes());
        }
        let _ = self.file.flush();
        self.path.clone()
    }
}

/// The directory's name: this user's, so users sharing a temporary directory never meet.
fn name() -> String {
    format!("llx-output-{}", uid())
}

fn uid() -> u32 {
    // SAFETY: getuid takes nothing, touches nothing and cannot fail.
    unsafe { libc::getuid() }
}

/// Remove what is older than [`KEEP_FOR`], then the oldest while the rest is over
/// [`MOST_IN_ALL`].
fn prune(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    let mut kept: Vec<(SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let held = entry.metadata().ok()?;
            let modified = held.modified().ok()?;
            Some((modified, held.len(), entry.path()))
        })
        .filter(|(_, _, path)| path.extension().is_some_and(|ext| ext == "log"))
        .filter(|(modified, _, path)| {
            let old = now
                .duration_since(*modified)
                .is_ok_and(|age| age > KEEP_FOR);
            !(old && fs::remove_file(path).is_ok())
        })
        .collect();
    kept.sort_unstable_by_key(|(modified, ..)| *modified);
    let mut total: u64 = kept.iter().map(|(_, size, _)| size).sum();
    for (_, size, path) in kept {
        if total <= MOST_IN_ALL {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total -= size;
        }
    }
}

#[cfg(test)]
#[path = "spill_tests.rs"]
mod tests;
