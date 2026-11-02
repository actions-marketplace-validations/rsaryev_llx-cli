//! A patch carried out: every hunk worked out first on the files as the hunks before it leave
//! them, the gate asked for each file it will change, and only then the files written. A hunk that
//! does not fit, a path outside the workspace or a refusal leaves every file as it was.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use contracts::gate::{Effect, Gate, Needs, Verdict};
use contracts::tool::{ToolError, ToolOutput};

use super::{Chunk, Hunk, NAME, apply, parse};
use crate::fs::atomic;
use crate::workspace::Workspace;

/// A file as the patch sees it.
struct File {
    path: PathBuf,
    shown: String,
    /// What is on disk, until the patch writes it.
    disk: Option<Vec<u8>>,
    /// What the hunks so far leave: `None` when there is no such file.
    now: Option<Vec<u8>>,
}

/// One thing the patch does to one file: what the gate is told, and what the report says.
struct Step {
    subject: String,
    summary: String,
    done: String,
    diff: String,
}

pub(super) fn run(
    workspace: &Workspace,
    gate: &dyn Gate,
    patch: &str,
) -> Result<ToolOutput, ToolError> {
    // Whole or nothing: a failure always leaves the files as they were, and says so, so the model
    // sends the whole patch again rather than the part it thinks is left.
    carry_out(workspace, gate, patch).map_err(|error| match error {
        ToolError::Failed(why) => ToolError::Failed(format!(
            "{}. No file was changed.",
            why.trim_end().trim_end_matches('.')
        )),
        fatal @ ToolError::Fatal(_) => fatal,
    })
}

fn carry_out(workspace: &Workspace, gate: &dyn Gate, patch: &str) -> Result<ToolOutput, ToolError> {
    let hunks = parse(patch).map_err(ToolError::Failed)?;
    let (mut files, mut steps) = (vec![], vec![]);
    for hunk in hunks {
        plan(workspace, &mut files, &mut steps, hunk)?;
    }
    for step in &steps {
        // No sandbox keeps a change inside a `.git` safe, the workspace's or a nested
        // repository's: a hook written there runs at the next commit (B3, B6). Any case: a
        // macOS volume takes `.GIT` as `.git`.
        let in_git = Path::new(&step.subject)
            .components()
            .any(|part| part.as_os_str().eq_ignore_ascii_case(".git"));
        let needs = match in_git {
            true => Needs::Open,
            false => Needs::Sandbox,
        };
        let effect = Effect {
            tool: NAME,
            subject: &step.subject,
            summary: &step.summary,
            needs,
        };
        match gate.check(effect) {
            Verdict::Allow | Verdict::Workspace => {}
            // A read-only verdict runs a command in the sandbox; a file written cannot be (B5).
            Verdict::ReadOnly => {
                return Err(ToolError::Failed(format!(
                    "not allowed: {} is read-only here, and a patch changes files",
                    step.subject
                )));
            }
            Verdict::Deny(reason) => return Err(crate::refused(&reason)),
        }
    }
    let changed: Vec<&File> = files.iter().filter(|f| f.now != f.disk).collect();
    for file in &changed {
        unchanged(&file.path, &file.shown, file.disk.as_deref())?;
    }
    write(&changed)?;
    // The model reads which files, as Codex's `apply_patch` says them; the diff is for the person
    // alone (`ToolOutput::shown`).
    let marks: Vec<&str> = (steps.iter())
        .filter(|step| !step.done.is_empty())
        .map(|step| step.done.as_str())
        .collect();
    let shown: Vec<String> = (steps.iter())
        .filter(|step| !step.diff.is_empty())
        .map(|step| format!("Diff of {}:\n{}", step.subject, step.diff))
        .collect();
    Ok(ToolOutput {
        report: format!(
            "Success. Updated the following files:\n{}",
            marks.join("\n")
        ),
        shown: shown.join("\n\n"),
    })
}

/// The writes of `changed` first, then its deletions, so a move that fails halfway keeps its
/// source. A failure puts back every file already written, from what was on disk.
fn write(changed: &[&File]) -> Result<(), ToolError> {
    let (written, removed): (Vec<&&File>, Vec<&&File>) =
        changed.iter().partition(|file| file.now.is_some());
    let mut done: Vec<&File> = vec![];
    // `bytes` at `path`, or no file there.
    let put = |path: &Path, bytes: &Option<Vec<u8>>| match bytes {
        Some(bytes) => atomic::write(path, bytes),
        None => fs::remove_file(path),
    };
    for file in written.into_iter().chain(removed) {
        if let Err(error) = put(&file.path, &file.now) {
            for back in done {
                let _ = put(&back.path, &back.disk);
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

/// The file `name`, read from disk the first time the patch names it.
fn load<'a>(
    workspace: &Workspace,
    files: &'a mut Vec<File>,
    name: &str,
) -> Result<&'a mut File, ToolError> {
    let path = workspace.writable(name)?;
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
        now: disk.clone(),
        disk,
    });
    Ok(&mut files[index])
}

/// What `hunk` makes of the files, noted in `files`, and the steps it takes.
fn plan(
    workspace: &Workspace,
    files: &mut Vec<File>,
    steps: &mut Vec<Step>,
    hunk: Hunk,
) -> Result<(), ToolError> {
    match hunk {
        Hunk::Add { path, content } => {
            let file = load(workspace, files, &path)?;
            if file.now.as_ref().is_some_and(|bytes| !bytes.is_empty()) {
                return Err(ToolError::Failed(format!(
                    "{} already exists and is not empty: change it with '*** Update File'",
                    file.shown
                )));
            }
            let size = count(content.lines().count());
            steps.push(Step {
                summary: format!("create {} ({size})", file.shown),
                done: format!("A {}", file.shown),
                ..Step::of(&file.shown)
            });
            file.now = Some(content.into_bytes());
        }
        Hunk::Delete { path } => {
            let file = load(workspace, files, &path)?;
            let Some(bytes) = file.now.take() else {
                return Err(ToolError::Failed(format!("{} does not exist", file.shown)));
            };
            let size = count(String::from_utf8_lossy(&bytes).lines().count());
            steps.push(Step {
                summary: format!("delete {} ({size})", file.shown),
                done: format!("D {}", file.shown),
                ..Step::of(&file.shown)
            });
        }
        Hunk::Update { path, to, chunks } => {
            update(workspace, files, steps, &path, to.as_deref(), &chunks)?;
        }
    }
    Ok(())
}

/// An update of `path` by `chunks`, and a move to `to` when there is one.
fn update(
    workspace: &Workspace,
    files: &mut Vec<File>,
    steps: &mut Vec<Step>,
    path: &str,
    to: Option<&str>,
    chunks: &[Chunk],
) -> Result<(), ToolError> {
    let file = load(workspace, files, path)?;
    let (shown, source) = (file.shown.clone(), file.path.clone());
    let Some(bytes) = file.now.take() else {
        return Err(ToolError::Failed(format!("{shown} does not exist")));
    };
    let old = String::from_utf8(bytes).map_err(|error| {
        ToolError::Failed(format!(
            "{shown} is not UTF-8 text ({}): it cannot be edited here",
            error.utf8_error()
        ))
    })?;
    let (new, diff) = apply::update(&old, chunks, &shown).map_err(ToolError::Failed)?;
    let moved = match to {
        Some(to) => move_to(workspace, files, to, &source, new)?,
        None => {
            file.now = Some(new.into_bytes());
            None
        }
    };
    let (added, removed) = apply::counts(chunks);
    let size = if chunks.is_empty() {
        String::new()
    } else {
        format!(" (+{added} -{removed} lines)")
    };
    let (done, summary) = match &moved {
        Some(dest) => (
            format!("D {shown}\nA {dest}"),
            format!("move {shown} to {dest}{size}"),
        ),
        None => (format!("M {shown}"), format!("update {shown}{size}")),
    };
    steps.push(Step {
        summary,
        done,
        diff,
        ..Step::of(&shown)
    });
    if let Some(dest) = moved {
        steps.push(Step {
            summary: format!("create {dest} (moved from {shown})"),
            ..Step::of(&dest)
        });
    }
    Ok(())
}

/// `new` put at `to` and the file at `source` (taken already) gone, when `to` is another, free
/// place; the name `to` is shown as, or none when the file stays where it is.
fn move_to(
    workspace: &Workspace,
    files: &mut Vec<File>,
    to: &str,
    source: &PathBuf,
    new: String,
) -> Result<Option<String>, ToolError> {
    let dest = load(workspace, files, to)?;
    if dest.path == *source {
        dest.now = Some(new.into_bytes());
        return Ok(None);
    }
    if dest.now.as_ref().is_some_and(|bytes| !bytes.is_empty()) {
        return Err(ToolError::Failed(format!(
            "{} already exists and is not empty: a move does not overwrite it",
            dest.shown
        )));
    }
    dest.now = Some(new.into_bytes());
    Ok(Some(dest.shown.clone()))
}

impl Step {
    /// A step on `shown`, its words still to say.
    fn of(shown: &str) -> Self {
        Self {
            subject: shown.to_string(),
            summary: String::new(),
            done: String::new(),
            diff: String::new(),
        }
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

/// Fail when the file is no longer what the patch was worked out from: the approval can take
/// minutes, and an editor, a formatter or the person may change the file meanwhile. Their change
/// is kept, and the model reads the file again.
fn unchanged(path: &Path, shown: &str, read: Option<&[u8]>) -> Result<(), ToolError> {
    if contents(path, shown)?.as_deref() == read {
        return Ok(());
    }
    Err(ToolError::Failed(format!(
        "{shown} changed while waiting for approval: read it again"
    )))
}

/// `1 line`, `12 lines`.
fn count(lines: usize) -> String {
    match lines {
        1 => "1 line".into(),
        n => format!("{n} lines"),
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
