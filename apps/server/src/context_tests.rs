use contracts::history::Turn;

use super::*;
use crate::instructions::{FILE_LIMIT, TOTAL_LIMIT};
use crate::testing::Scratch;

fn environment(root: &Path) -> Environment<'_> {
    Environment {
        root,
        mode: "default: the person approves each command",
        now: 1_792_627_200,
        runtimes: &[],
    }
}

#[test]
fn the_runtimes_on_the_path_are_named_in_a_fixed_order_and_only_those() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new();
    let program = |dir: &str, name: &str, mode: u32| {
        let path = scratch.file(&format!("{dir}/{name}"), "#!/bin/sh\n");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    program("a", "bun", 0o755);
    program("b", "python3", 0o755);
    // Not a program: no one can run it.
    program("b", "node", 0o644);
    let path = std::env::join_paths([scratch.path().join("a"), scratch.path().join("b")]).unwrap();
    assert_eq!(runtimes(&path), ["python3", "bun"]);
    assert!(runtimes(std::ffi::OsStr::new("")).is_empty());
}

#[test]
fn the_environment_names_the_runtimes_found_and_says_nothing_when_there_are_none() {
    let scratch = Scratch::new();
    let found = Environment {
        runtimes: &["python3", "node"],
        ..environment(scratch.path())
    };
    assert!(
        describe(&found).contains("\nRuntimes: python3, node (for a script: use whichever"),
        "{}",
        describe(&found)
    );
    assert!(!describe(&environment(scratch.path())).contains("Runtimes"));
}

#[test]
fn instructions_run_from_the_repository_root_down_to_the_workspace() {
    let scratch = Scratch::new();
    std::fs::create_dir_all(scratch.path().join("repo/.git")).unwrap();
    scratch.file("AGENTS.md", "outside the repository");
    scratch.file("repo/AGENTS.md", "root rules");
    scratch.file("repo/app/AGENTS.md", "app rules");
    scratch.file("repo/app/web/README.md", "not instructions");
    let workspace = scratch.path().join("repo/app/web");
    let found = instructions(&workspace);
    assert!(!found.contains("outside the repository"));
    let (root, app) = (
        found.find("root rules").unwrap(),
        found.find("app rules").unwrap(),
    );
    assert!(root < app, "the deeper file comes later: {found}");
    assert!(found.contains(&format!(
        "<project_instructions path=\"{}\">",
        scratch.path().join("repo/AGENTS.md").display()
    )));
    assert_eq!(found.matches("</project_instructions>").count(), 2);
}

#[test]
fn outside_a_repository_only_the_workspace_counts() {
    // Inside a repository the walk rightly finds it, so this needs a directory outside any.
    let Some(scratch) = Scratch::outside_repository() else {
        eprintln!(
            "SKIPPED outside_a_repository_only_the_workspace_counts: every temporary directory \
             is inside a git repository"
        );
        return;
    };
    scratch.file("AGENTS.md", "parent rules");
    scratch.file("work/AGENTS.md", "work rules");
    let found = instructions(&scratch.path().join("work"));
    assert!(found.contains("work rules"));
    assert!(!found.contains("parent rules"));
    assert_eq!(instructions(&scratch.path().join("nowhere")), "");
}

#[test]
fn instructions_are_capped_per_file_and_in_all() {
    let scratch = Scratch::new();
    std::fs::create_dir_all(scratch.path().join(".git")).unwrap();
    scratch.file("AGENTS.md", &"a".repeat(FILE_LIMIT + 10));
    scratch.file("b/AGENTS.md", &"b".repeat(FILE_LIMIT + 10));
    scratch.file("b/c/AGENTS.md", "c rules");
    let found = instructions(&scratch.path().join("b/c"));
    assert!(found.contains(&"a".repeat(FILE_LIMIT)));
    assert!(!found.contains(&"a".repeat(FILE_LIMIT + 1)));
    assert_eq!(found.matches("[10 more bytes not shown").count(), 2);
    assert!(found.len() < TOTAL_LIMIT + 1024, "{} bytes", found.len());
    assert!(
        !found.contains("c rules"),
        "nothing is left for a third file"
    );
}

#[test]
fn a_cut_never_splits_a_character() {
    assert_eq!(cut("h\u{e9}llo", 2), "h");
    assert_eq!(cut("short", 10), "short");
}

#[test]
fn the_environment_says_where_when_and_how() {
    let scratch = Scratch::new();
    let text = gather(&environment(scratch.path()));
    assert!(text.starts_with("<environment>\n"));
    assert!(text.contains(&format!("Working directory: {}", scratch.path().display())));
    assert!(text.contains("Date: 2026-10-22 (UTC)"));
    assert!(text.contains("Mode: default: the person approves each command"));
    assert!(!text.contains("Approvals:"));
    assert!(text.trim_end().ends_with("</environment>"));
}

#[test]
fn dates_are_civil() {
    assert_eq!(date(0), "1970-01-01");
    assert_eq!(date(951_782_400), "2000-02-29");
    assert_eq!(date(1_792_627_200 + 86_399), "2026-10-22");
}

#[test]
fn a_resumed_session_tells_what_was_asked_before() {
    let scratch = Scratch::new();
    let turn = |goal: &str, answer: &str| Turn {
        goal: goal.into(),
        answer: answer.into(),
        status: "done".into(),
        at: 1,
        steps: vec![],
    };
    let session = Session {
        id: "1-ab".into(),
        turns: vec![
            turn("fix the parser", "Fixed it."),
            turn("add a test", "Added."),
        ],
        ..Session::default()
    };
    let text = going_on(scratch.path(), &session);
    assert!(text.contains("<previous_session id=\"1-ab\">"));
    assert!(text.contains("1. Task: fix the parser\n   Ended: done\n   Answer: Fixed it."));
    assert!(text.contains("2. Task: add a test"));
    let empty = Session::default();
    assert!(!going_on(scratch.path(), &empty).contains("previous_session"));
}

fn turn(goal: &str, answer: &str) -> Turn {
    Turn {
        goal: goal.into(),
        answer: answer.into(),
        status: "done".into(),
        at: 1,
        steps: vec![],
    }
}

#[test]
fn a_huge_earlier_task_hides_neither_itself_nor_the_others() {
    let scratch = Scratch::new();
    let piped = format!(
        "fix it\n\n<stdin>\n{}\n</stdin>",
        "log line\n".repeat(12_000)
    );
    let session = Session {
        id: "1-ab".into(),
        turns: vec![
            turn("first task", "One."),
            turn(&"x".repeat(100_000), "Two."),
            turn(&piped, "Three."),
        ],
        ..Session::default()
    };
    let text = going_on(scratch.path(), &session);
    assert!(text.contains("1. Task: first task"), "{text}");
    assert!(text.contains(&format!("2. Task: {}…", "x".repeat(GOAL_LIMIT))));
    assert!(text.contains("3. Task: fix it [with stdin, not kept here]"));
    assert!(!text.contains("log line"));
}

#[test]
fn a_session_from_another_workspace_says_where_it_ran() {
    let scratch = Scratch::new();
    let here = scratch.path().display().to_string();
    let mut session = Session {
        id: "1-ab".into(),
        workspace: "/elsewhere/repo".into(),
        turns: vec![turn("fix the parser", "Fixed it.")],
        ..Session::default()
    };
    let text = going_on(scratch.path(), &session);
    assert!(text.contains("Its tasks ran in /elsewhere/repo, not in this workspace"));
    session.workspace = here.clone();
    let text = going_on(scratch.path(), &session);
    assert!(text.contains(&format!("Its tasks ran in this workspace, {here}.")));
}

#[test]
fn a_saved_session_resumed_from_the_fresh_context_follows_the_instructions() {
    let scratch = Scratch::new();
    scratch.file("AGENTS.md", "rules");
    let session = Session {
        id: "1-ab".into(),
        turns: vec![turn("fix the parser", "Fixed it.")],
        ..Session::default()
    };
    let fresh = gather(&environment(scratch.path()));
    let text = resumed(&fresh, &session, scratch.path());
    assert!(text.starts_with(&fresh));
    assert!(text.find("rules") < text.find("<previous_session"));
    let empty = Session::default();
    assert_eq!(resumed(&fresh, &empty, scratch.path()), fresh);
}

/// The context of a run that resumes `session`.
fn going_on(root: &Path, session: &Session) -> String {
    resumed(&gather(&environment(root)), session, root)
}
