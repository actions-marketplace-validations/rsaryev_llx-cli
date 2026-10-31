//! Writing a file whole or not at all: the content goes to a temporary file beside it, which then
//! replaces it in one rename. A reader (a build, a test watcher) never sees half a file, and a
//! failed write leaves the old one as it was.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Bytes of the file's name kept in a temporary name: a long name plus the suffix must still fit
/// in a directory entry (255 bytes on most systems).
const STEM: usize = 100;
/// Temporary names tried before giving up: each one taken is skipped, never opened.
const TRIES: usize = 16;

/// Replace `path` with `content`, creating its parent directories; an existing file keeps its
/// permissions (an executable script stays executable).
///
/// A read-only file is refused: a rename needs only the directory's permission, and would replace
/// a file someone locked on purpose. A file with other hard links is written in place instead,
/// so every name keeps the new content.
///
/// Not synced to disk: the rename already makes the change atomic for readers, and a sync per
/// edit would cost more than a crash at that moment would.
pub(super) fn write(path: &Path, content: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent directory"))?;
    fs::create_dir_all(parent)?;
    let meta = fs::metadata(path).ok();
    if meta
        .as_ref()
        .is_some_and(|meta| meta.permissions().readonly())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "it is read-only",
        ));
    }
    if meta.as_ref().is_some_and(linked) {
        return OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)?
            .write_all(content);
    }
    let (temporary, mut file) = create(|| temporary(path))?;
    let written = (|| {
        file.write_all(content)?;
        drop(file);
        if let Some(meta) = meta {
            fs::set_permissions(&temporary, meta.permissions())?;
        }
        fs::rename(&temporary, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written
}

/// Whether the file has names besides this one, which a rename would leave on the old content.
fn linked(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.nlink() > 1
}

/// A new temporary file at the first free name `next` gives. Each name is created, never opened
/// as found: a file or a symlink already there (planted by a checkout, left by a command) is
/// skipped, so the write can never land on whatever it points to. The names are passed in so a
/// test knows them: the counter behind [`temporary`] is shared by every write in the process.
fn create(mut next: impl FnMut() -> PathBuf) -> io::Result<(PathBuf, File)> {
    let mut last = None;
    for _ in 0..TRIES {
        let temporary = next();
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => last = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("no temporary name was free")))
}

/// A name beside `path` that no other write uses: the process and a counter tell writes apart.
fn temporary(path: &Path) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut end = name.len().min(STEM);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    path.with_file_name(format!(
        ".{}.{}-{}.tmp",
        name.get(..end).unwrap_or_default(),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

#[cfg(test)]
#[path = "atomic_tests.rs"]
mod tests;
