use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use contracts::gate::{Effect, Gate, Open, Verdict};
use contracts::tool::{Tool, ToolError, ToolOutput};
use serde_json::Value;

use crate::fs::PatchTool;
use crate::testing::{Recorder, Scratch};
use crate::workspace::Workspace;

fn patch(dir: &Scratch, gate: &dyn Gate, body: &str) -> Result<String, ToolError> {
    patch_output(dir, gate, body).map(|output| output.report)
}

#[expect(
    clippy::unwrap_in_result,
    reason = "a test's helper: a setup that fails, fails the test"
)]
fn patch_output(dir: &Scratch, gate: &dyn Gate, body: &str) -> Result<ToolOutput, ToolError> {
    let workspace = Workspace::new(dir.path()).unwrap();
    let tool = PatchTool {
        workspace: &workspace,
        gate,
    };
    let text = format!("*** Begin Patch\n{body}*** End Patch");
    tool.call(&Value::String(text))
}

fn failed(result: Result<String, ToolError>) -> String {
    match result {
        Err(ToolError::Failed(message)) => message,
        other => panic!("expected Failed, got {other:?}"),
    }
}

fn read(dir: &Scratch, name: &str) -> String {
    fs::read_to_string(dir.path().join(name)).unwrap()
}

/// Refuses the file it is told to, allows the rest.
struct Refusing(&'static str);

impl Gate for Refusing {
    fn check(&self, effect: Effect) -> Verdict {
        if effect.subject == self.0 {
            Verdict::Deny("no".into())
        } else {
            Verdict::Allow
        }
    }
}

#[test]
fn a_patch_updates_creates_deletes_and_moves_files() {
    let dir = Scratch::new();
    dir.file("a.txt", "one\ntwo\nthree\n");
    dir.file("gone.txt", "bye\n");
    dir.file("old/b.txt", "keep\nme\n");
    let gate = Recorder::default();
    let report = patch(
        &dir,
        &gate,
        "*** Update File: a.txt\n@@\n one\n-two\n+2\n+2b\n three\n\
         *** Add File: sub/new.txt\n+hello\n\
         *** Delete File: gone.txt\n\
         *** Update File: old/b.txt\n*** Move to: new/b.txt\n@@\n-me\n+you\n",
    )
    .unwrap();
    assert_eq!(read(&dir, "a.txt"), "one\n2\n2b\nthree\n");
    assert_eq!(read(&dir, "sub/new.txt"), "hello\n");
    assert!(!dir.path().join("gone.txt").exists());
    assert!(!dir.path().join("old/b.txt").exists());
    assert_eq!(read(&dir, "new/b.txt"), "keep\nyou\n");
    // Codex's words: which files, and nothing of what changed in them.
    assert_eq!(
        report,
        "Success. Updated the following files:\nM a.txt\nA sub/new.txt\nD gone.txt\n\
         D old/b.txt\nA new/b.txt"
    );
}

#[test]
fn the_diff_is_for_the_person_and_never_in_the_report() {
    let dir = Scratch::new();
    dir.file("a.txt", "one\ntwo\nthree\n");
    let output = patch_output(
        &dir,
        &Open,
        "*** Update File: a.txt\n@@\n one\n-two\n+2\n three\n",
    )
    .unwrap();
    assert!(!output.report.contains("two"), "{}", output.report);
    assert!(
        output
            .shown
            .starts_with("Diff of a.txt:\n@@ -1,3 +1,3 @@\n one\n-two\n+2\n three"),
        "{}",
        output.shown
    );
}

#[test]
fn edits_far_apart_show_what_each_one_did() {
    let dir = Scratch::new();
    let middle = "same\n".repeat(1_100);
    dir.file("a.txt", format!("a\n{middle}z\n"));
    let output = patch_output(
        &dir,
        &Open,
        "*** Update File: a.txt\n@@\n-a\n+A\n@@\n same\n-z\n+Z\n",
    )
    .unwrap();
    assert_eq!(read(&dir, "a.txt"), format!("A\n{middle}Z\n"));
    let diff = output.shown;
    assert!(diff.starts_with("Diff of a.txt:\n@@ -1,4 +1,4 @@\n-a\n+A\n same\n"));
    assert!(diff.contains("@@ -1099,4 +1099,4 @@\n same\n same\n same\n-z\n+Z\n"));
    assert!(
        !diff.contains("-same") && !diff.contains("truncated"),
        "{diff}"
    );
}

#[test]
fn the_gate_is_asked_for_each_file_before_anything_is_written() {
    let dir = Scratch::new();
    dir.file("a.txt", "x\n");
    dir.file("c.txt", "c1\nc2\n");
    let gate = Recorder::default();
    patch(
        &dir,
        &gate,
        "*** Update File: a.txt\n@@\n-x\n+y\n*** Add File: b.txt\n+1\n+2\n\
         *** Delete File: c.txt\n\
         *** Update File: a.txt\n*** Move to: d.txt\n",
    )
    .unwrap();
    assert_eq!(
        *gate.asked.lock().unwrap(),
        [
            "apply_patch: update a.txt (+1 -1 lines)",
            "apply_patch: create b.txt (2 lines)",
            "apply_patch: delete c.txt (2 lines)",
            "apply_patch: move a.txt to d.txt",
            "apply_patch: create d.txt (moved from a.txt)",
        ]
    );
}

#[test]
fn the_subject_of_each_question_is_the_file() {
    struct Subjects(Mutex<Vec<String>>);
    impl Gate for Subjects {
        fn check(&self, effect: Effect) -> Verdict {
            self.0.lock().unwrap().push(effect.subject.into());
            Verdict::Allow
        }
    }
    let dir = Scratch::new();
    dir.file("a.txt", "x\n");
    let gate = Subjects(Mutex::default());
    patch(
        &dir,
        &gate,
        "*** Update File: a.txt\n*** Move to: b/c.txt\n*** Add File: d.txt\n+1\n",
    )
    .unwrap();
    assert_eq!(*gate.0.lock().unwrap(), ["a.txt", "b/c.txt", "d.txt"]);
}

#[test]
fn a_file_the_gate_refuses_stops_the_whole_patch() {
    let dir = Scratch::new();
    dir.file("a.txt", "x\n");
    dir.file("b.txt", "y\n");
    let message = failed(patch(
        &dir,
        &Refusing("b.txt"),
        "*** Update File: a.txt\n@@\n-x\n+X\n*** Update File: b.txt\n@@\n-y\n+Y\n\
         *** Add File: c.txt\n+c\n",
    ));
    assert!(message.contains("not allowed: no"), "{message}");
    assert_eq!(read(&dir, "a.txt"), "x\n");
    assert_eq!(read(&dir, "b.txt"), "y\n");
    assert!(!dir.path().join("c.txt").exists());
}

#[test]
fn a_hunk_that_does_not_match_writes_nothing_and_asks_nothing() {
    let dir = Scratch::new();
    dir.file("a.txt", "x\n");
    dir.file("b.txt", "y\n");
    let gate = Recorder::default();
    let message = failed(patch(
        &dir,
        &gate,
        "*** Update File: a.txt\n@@\n-x\n+X\n*** Update File: b.txt\n@@\n-nope\n+Y\n",
    ));
    assert!(
        message.contains("b.txt") && message.contains("nope"),
        "{message}"
    );
    assert_eq!(read(&dir, "a.txt"), "x\n");
    assert!(gate.asked.lock().unwrap().is_empty());
}

#[test]
fn later_hunks_see_the_files_as_earlier_ones_leave_them() {
    let dir = Scratch::new();
    dir.file("a.txt", "1\n2\n");
    patch(
        &dir,
        &Open,
        "*** Update File: a.txt\n@@\n-1\n+one\n*** Add File: b.txt\n+b\n\
         *** Update File: a.txt\n@@\n-2\n+two\n*** Update File: b.txt\n@@\n-b\n+B\n",
    )
    .unwrap();
    assert_eq!(read(&dir, "a.txt"), "one\ntwo\n");
    assert_eq!(read(&dir, "b.txt"), "B\n");
}

#[test]
fn a_path_outside_the_workspace_refuses_the_patch() {
    let dir = Scratch::new();
    let inside = dir.path().join("inside");
    fs::create_dir_all(&inside).unwrap();
    fs::write(inside.join("a.txt"), "x\n").unwrap();
    let workspace = Workspace::new(&inside).unwrap();
    let tool = PatchTool {
        workspace: &workspace,
        gate: &Open,
    };
    let text = "*** Begin Patch\n*** Update File: a.txt\n@@\n-x\n+X\n\
                *** Add File: ../outside.txt\n+o\n*** End Patch";
    let message = match tool.call(&Value::String(text.into())) {
        Err(ToolError::Failed(message)) => message,
        other => panic!("{other:?}"),
    };
    assert!(message.contains("outside the workspace"), "{message}");
    assert_eq!(fs::read_to_string(inside.join("a.txt")).unwrap(), "x\n");
    assert!(!dir.path().join("outside.txt").exists());
}

#[test]
fn what_would_overwrite_or_is_not_there_is_refused() {
    let dir = Scratch::new();
    dir.file("a.txt", "x\n");
    dir.file("b.txt", "y\n");
    dir.file("sub/c.txt", "z\n");
    for (body, expected) in [
        ("*** Add File: a.txt\n+n\n", "already exists"),
        ("*** Delete File: nothing.txt\n", "does not exist"),
        (
            "*** Update File: nothing.txt\n@@\n-a\n+b\n",
            "does not exist",
        ),
        (
            "*** Update File: a.txt\n*** Move to: b.txt\n",
            "already exists",
        ),
        ("*** Delete File: sub\n", "directory"),
    ] {
        let message = failed(patch(&dir, &Open, body));
        assert!(message.contains(expected), "{body}: {message}");
    }
    assert_eq!(read(&dir, "a.txt"), "x\n");
    assert_eq!(read(&dir, "b.txt"), "y\n");
}

#[test]
fn a_file_changed_while_the_gate_waited_is_not_overwritten() {
    struct Slow(PathBuf);
    impl Gate for Slow {
        fn check(&self, _effect: Effect) -> Verdict {
            fs::write(&self.0, "someone else\n").unwrap();
            Verdict::Allow
        }
    }
    let dir = Scratch::new();
    let path = dir.file("a.txt", "x\n");
    let message = failed(patch(
        &dir,
        &Slow(path),
        "*** Update File: a.txt\n@@\n-x\n+X\n",
    ));
    assert!(message.contains("changed while waiting"), "{message}");
    assert_eq!(read(&dir, "a.txt"), "someone else\n");
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test's fixture: the command ends by itself"
)]
#[test]
fn a_fifo_is_refused_before_it_is_opened() {
    let dir = Scratch::new();
    let made = std::process::Command::new("mkfifo")
        .arg(dir.path().join("pipe"))
        .status()
        .unwrap();
    assert!(made.success());
    let gate = Recorder::default();
    let message = failed(patch(&dir, &gate, "*** Update File: pipe\n@@\n-a\n+b\n"));
    assert_eq!(
        message,
        "pipe is not a regular file: apply_patch changes files. No file was changed."
    );
    assert!(gate.asked.lock().unwrap().is_empty());
    dir.file("sub/a.txt", "a\n");
    let message = failed(patch(&dir, &gate, "*** Add File: sub\n+b\n"));
    assert_eq!(
        message,
        "sub is a directory: apply_patch changes files. No file was changed."
    );
}

/// What a read-only mode answers: a command runs in the sandbox, but a file written cannot be.
struct ReadOnly;

impl Gate for ReadOnly {
    fn check(&self, _: Effect) -> Verdict {
        Verdict::ReadOnly
    }
}

#[test]
fn a_read_only_verdict_refuses_the_patch_and_leaves_every_file_as_it_was() {
    let dir = Scratch::new();
    dir.file("notes.txt", "one\n");
    let message = failed(patch(
        &dir,
        &ReadOnly,
        "*** Update File: notes.txt\n@@\n-one\n+two\n*** Add File: new.txt\n+new\n",
    ));
    assert!(message.contains("read-only"), "{message}");
    assert_eq!(read(&dir, "notes.txt"), "one\n");
    assert!(!dir.path().join("new.txt").exists());
}

#[test]
fn a_move_to_the_same_place_is_an_update() {
    let dir = Scratch::new();
    fs::write(dir.path().join("a.txt"), "x\n").unwrap();
    let report = patch(
        &dir,
        &Open,
        "*** Update File: a.txt\n*** Move to: a.txt\n@@\n-x\n+y\n",
    )
    .unwrap();
    assert_eq!(report, "Success. Updated the following files:\nM a.txt");
    assert_eq!(read(&dir, "a.txt"), "y\n");
}

#[test]
fn a_write_that_fails_puts_back_every_file_written_before_it() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new();
    dir.file("a.txt", "x\n");
    dir.file("b.txt", "y\n");
    fs::set_permissions(dir.path().join("b.txt"), fs::Permissions::from_mode(0o444)).unwrap();
    let message = failed(patch(
        &dir,
        &Open,
        "*** Add File: c.txt\n+new\n*** Update File: a.txt\n@@\n-x\n+X\n*** Update File: b.txt\n@@\n-y\n+Y\n",
    ));
    assert!(
        message.contains("b.txt") && message.ends_with("No file was changed."),
        "{message}"
    );
    assert_eq!(read(&dir, "a.txt"), "x\n");
    assert!(!dir.path().join("c.txt").exists());
}

#[test]
fn every_failure_of_a_patch_says_no_file_was_changed() {
    let dir = Scratch::new();
    dir.file("a.txt", "x\n");
    let message = failed(patch(
        &dir,
        &Open,
        "*** Update File: a.txt\n@@\n-nope\n+Y\n",
    ));
    assert!(message.ends_with("No file was changed."), "{message}");
    let message = failed(patch(
        &dir,
        &Refusing("a.txt"),
        "*** Update File: a.txt\n@@\n-x\n+Y\n",
    ));
    assert!(message.ends_with("No file was changed."), "{message}");
}

#[test]
fn a_workspace_verdict_makes_the_change_and_one_inside_git_must_leave_the_sandbox() {
    let dir = Scratch::new();
    dir.file("notes.txt", "one\n");
    dir.file(".git/config", "[core]\n");
    let gate = Recorder::answering(Verdict::Workspace);
    let body =
        "*** Update File: notes.txt\n@@\n-one\n+two\n*** Add File: .git/hooks/pre-commit\n+x\n";
    patch(&dir, &gate, body).unwrap();
    assert_eq!(read(&dir, "notes.txt"), "two\n");
    assert_eq!(
        *gate.asked.lock().unwrap(),
        [
            "apply_patch: update notes.txt (+1 -1 lines)",
            "apply_patch: create .git/hooks/pre-commit (1 line) [open]"
        ]
    );
}

#[test]
fn a_change_inside_a_nested_repositorys_git_must_leave_the_sandbox_too() {
    let dir = Scratch::new();
    dir.file("sub/.git/config", "[core]\n");
    let gate = Recorder::answering(Verdict::Workspace);
    patch(&dir, &gate, "*** Add File: sub/.git/hooks/pre-commit\n+x\n").unwrap();
    assert_eq!(
        *gate.asked.lock().unwrap(),
        ["apply_patch: create sub/.git/hooks/pre-commit (1 line) [open]"]
    );
}
