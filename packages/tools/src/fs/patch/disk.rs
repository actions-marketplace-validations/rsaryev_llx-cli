//! The files of a patch as it found them on disk and as it leaves them: each read once, the first
//! time the patch names it, read again before anything is written, then written whole, or every
//! one put back as it was. A symlink the patch deletes is the link itself, never what it points at.

use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use contracts::tool::ToolError;

use super::NAME;
use crate::fs::atomic;
use crate::workspace::Workspace;

/// A file as the patch sees it.
pub(super) struct File {
    pub(super) path: PathBuf,
    pub(super) shown: String,
    /// Where the symlink at `path` points, when the patch deletes the link itself: its `now`
    /// holds no bytes of it, only that it is there.
    pub(super) link: Option<PathBuf>,
    /// What is on disk, until the patch writes it.
    pub(super) disk: Option<Vec<u8>>,
    /// What the hunks so far leave: `None` when there is no such file.
    pub(super) now: Option<Vec<u8>>,
}

/// The file `name`, read from disk the first time the patch names it.
pub(super) fn load<'a>(
    workspace: &Workspace,
    files: &'a mut Vec<File>,
    name: &str,
) -> Result<&'a mut File, ToolError> {
    let path = match unlinked(workspace, files, name) {
        Some(place) => place,
        None => workspace.writable(name)?,
    };
    if let Some(index) = files.iter().position(|file| file.path == path) {
        return Ok(&mut files[index]);
    }
    let shown = workspace.shown(&path);
    // A FIFO or a device would block the read, or never end it.
    if let Some(meta) = fs::metadata(&path).ok().filter(|meta| !meta.is_file()) {
        let what = match meta.is_dir() {
            true => "a directory",
            false => "not a regular file",
        };
        return Err(ToolError::Failed(format!(
            "{shown} is {what}: {NAME} changes files"
        )));
    }
    let disk = contents(&path, &shown)?;
    let index = files.len();
    files.push(File {
        path,
        shown,
        link: None,
        now: disk.clone(),
        disk,
    });
    Ok(&mut files[index])
}

/// The file `*** Delete File: name` removes: the link itself when `name` is a symlink (to a
/// file, a directory or nothing), as `rm` takes it; else the file, as [`load`] has it.
pub(super) fn deleted<'a>(
    workspace: &Workspace,
    files: &'a mut Vec<File>,
    name: &str,
) -> Result<&'a mut File, ToolError> {
    let place = workspace.removable(name)?;
    let Ok(target) = fs::read_link(&place) else {
        return load(workspace, files, name);
    };
    if let Some(index) = files.iter().position(|file| file.path == place) {
        return Ok(&mut files[index]);
    }
    let index = files.len();
    files.push(File {
        shown: workspace.shown(&place),
        path: place,
        link: Some(target),
        disk: None,
        now: Some(vec![]),
    });
    Ok(&mut files[index])
}

/// Where the link `name` was, when the patch deleted it: the name is that place now, as it is
/// after `rm`, not what the link pointed at.
fn unlinked(workspace: &Workspace, files: &[File], name: &str) -> Option<PathBuf> {
    if !files.iter().any(|file| file.link.is_some()) {
        return None;
    }
    let place = workspace.removable(name).ok()?;
    (files.iter())
        .any(|file| file.link.is_some() && file.path == place)
        .then_some(place)
}

/// The files the patch changes: their bytes, or a link that goes.
pub(super) fn changed(files: &[File]) -> Vec<&File> {
    (files.iter())
        .filter(|file| file.link.is_some() || file.now != file.disk)
        .collect()
}

/// Fail when the file is no longer what the patch was worked out from: the approval can take
/// minutes, and an editor, a formatter or the person may change the file meanwhile. Their change
/// is kept, and the model reads the file again.
pub(super) fn unchanged(file: &File) -> Result<(), ToolError> {
    let same = match &file.link {
        Some(target) => fs::read_link(&file.path).is_ok_and(|now| now == *target),
        None => contents(&file.path, &file.shown)?.as_deref() == file.disk.as_deref(),
    };
    if same {
        return Ok(());
    }
    Err(ToolError::Failed(format!(
        "{} changed while waiting for approval: read it again",
        file.shown
    )))
}

/// The writes of `changed` first, then its deletions, so a move that fails halfway keeps its
/// source. A failure puts back every file already written, as it was on disk.
pub(super) fn write(changed: &[&File]) -> Result<(), ToolError> {
    let (written, removed): (Vec<&File>, Vec<&File>) =
        changed.iter().copied().partition(|file| file.now.is_some());
    let mut done: Vec<&File> = vec![];
    for file in written.into_iter().chain(removed) {
        if let Err(error) = put(file) {
            for back in done {
                let _ = put_back(back);
            }
            return Err(ToolError::Failed(format!(
                "{} could not be written: {error}",
                file.shown
            )));
        }
        done.push(file);
    }
    Ok(())
}

/// `file` as the patch leaves it: its bytes, or no file. A link the patch deleted goes first, so
/// what takes its place is never written through it, and comes back when that cannot be written.
fn put(file: &File) -> io::Result<()> {
    match (&file.link, &file.now) {
        (_, None) => fs::remove_file(&file.path),
        (None, Some(bytes)) => atomic::write(&file.path, bytes),
        (Some(_), Some(bytes)) => {
            fs::remove_file(&file.path)?;
            atomic::write(&file.path, bytes).inspect_err(|_| {
                let _ = put_back(file);
            })
        }
    }
}

/// `file` as it was on disk: its bytes, no file, or the link it was.
fn put_back(file: &File) -> io::Result<()> {
    match (&file.link, &file.disk) {
        (Some(target), _) => {
            // What the patch put in its place, if anything.
            let _ = fs::remove_file(&file.path);
            symlink(target, &file.path)
        }
        (None, Some(bytes)) => atomic::write(&file.path, bytes),
        (None, None) => fs::remove_file(&file.path),
    }
}

/// The file's bytes, or `None` when there is no file yet.
fn contents(path: &Path, shown: &str) -> Result<Option<Vec<u8>>, ToolError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ToolError::Failed(format!(
            "{shown} could not be read: {error}"
        ))),
    }
}
