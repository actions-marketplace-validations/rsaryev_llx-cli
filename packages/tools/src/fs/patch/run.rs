//! A patch carried out: every hunk worked out first on the files as the hunks before it leave
//! them, the gate asked for each file it will change, and only then the files written. A hunk that
//! does not fit, a path outside the workspace or a refusal leaves every file as it was.

use std::path::Path;

use contracts::gate::{Effect, Gate, Needs, Verdict};
use contracts::tool::{ToolError, ToolOutput};

use super::disk::{self, File, load};
use super::{Chunk, Hunk, NAME, apply, parse};
use crate::workspace::Workspace;

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
    // Every hunk is tried, so each file whose lines do not match is named at once: a patch of
    // many files is mended in one retry, not one file a turn.
    let mut missed: Vec<String> = Vec::new();
    for hunk in hunks {
        match plan(workspace, &mut files, &mut steps, hunk) {
            Ok(()) => {}
            Err(ToolError::Failed(why)) => missed.push(why.trim_end().to_string()),
            Err(fatal @ ToolError::Fatal(_)) => return Err(fatal),
        }
    }
    if !missed.is_empty() {
        return Err(ToolError::Failed(missed.join("\n\n")));
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
    let changed = disk::changed(&files);
    for file in &changed {
        disk::unchanged(file)?;
    }
    disk::write(&changed)?;
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
            let file = disk::deleted(workspace, files, &path)?;
            let Some(bytes) = file.now.take() else {
                return Err(ToolError::Failed(format!("{} does not exist", file.shown)));
            };
            let summary = match &file.link {
                Some(target) => format!(
                    "delete the symlink {} (to {})",
                    file.shown,
                    target.display()
                ),
                None => format!(
                    "delete {} ({})",
                    file.shown,
                    count(String::from_utf8_lossy(&bytes).lines().count())
                ),
            };
            steps.push(Step {
                summary,
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

/// An update of `path` by `chunks`, and a move to `to` when there is one: through a symlink, an
/// update changes what it points at, and a move takes the link.
fn update(
    workspace: &Workspace,
    files: &mut Vec<File>,
    steps: &mut Vec<Step>,
    path: &str,
    to: Option<&str>,
    chunks: &[Chunk],
) -> Result<(), ToolError> {
    let (file, link) = match to {
        Some(to) => disk::moved(workspace, files, path, to)?,
        None => (load(workspace, files, path)?, None),
    };
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
    let (new, diff) = match apply::update(&old, chunks, &shown) {
        Ok(changed) => changed,
        Err(why) => {
            // As it was, for a later hunk of the same file and for what is reported.
            file.now = Some(old.into_bytes());
            return Err(ToolError::Failed(why));
        }
    };
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
    let (done, summary) = match (&moved, link) {
        (Some(dest), Some(target)) => (
            format!("D {shown}\nA {dest}"),
            format!(
                "move the symlink {shown} (to {}) to {dest}{size}",
                target.display()
            ),
        ),
        (Some(dest), None) => (
            format!("D {shown}\nA {dest}"),
            format!("move {shown} to {dest}{size}"),
        ),
        (None, _) => (format!("M {shown}"), format!("update {shown}{size}")),
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
    source: &Path,
    new: String,
) -> Result<Option<String>, ToolError> {
    let dest = load(workspace, files, to)?;
    if dest.path == source {
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
