//! The terminals of one daemon stay apart (docs/daemon.md): each has its own questions, its own
//! command line, its own sessions; and the daemon is not replaced while any of them, or the
//! work one moved to the background, still needs it.

use serde_json::json;

use super::LONG;
use super::daemon::{WAIT, backgrounded, status, status_with};
use crate::common::model::{Model, Reply, said};
use crate::common::term::{Term, key};

#[test]
fn a_question_open_at_one_terminal_does_not_hold_another_s() {
    let model = Model::start(vec![
        Reply::call(
            "bash",
            json!({"command": "echo first-one", "escalate": true}),
        ),
        Reply::call(
            "bash",
            json!({"command": "echo second-one", "escalate": true}),
        ),
    ]);
    let mut first = Term::start(&model, &[]);
    first.type_text("ask me").press(key::ENTER);
    first.wait_for("Run this command?", LONG);
    let mut second = first.beside(&model, &[]);
    second.type_text("ask me too").press(key::ENTER);
    let screen = second.wait_for("Run this command?", LONG);
    assert!(screen.contains("echo second-one"), "{screen}");
}

#[test]
fn a_terminal_that_moved_its_sessions_away_is_no_longer_attached() {
    let model = Model::start(vec![Reply::call("bash", json!({"command": WAIT}))]);
    let mut first = Term::start(&model, &["-y"]);
    backgrounded(&mut first);
    let said = status_with(&first, "0 attached");
    assert!(said.contains("0 attached, 1 sessions open"), "{said}");
}

#[test]
fn a_daemon_of_another_build_is_kept_while_a_terminal_is_attached() {
    let model = Model::start(vec![]);
    let mut older = Term::spawn(&model, &[], &[("LLX_BUILD", "0.0.0+older")]);
    older.wait_for("❯", LONG);
    let newer = older.beside(&model, &[]);
    let said = String::from_utf8_lossy(&newer.raw()).into_owned();
    assert!(said.contains("this session runs in this process"), "{said}");
    assert!(status(&newer).contains("build 0.0.0+older, 1 attached"));
    assert_eq!(older.exited(std::time::Duration::from_millis(300)), None);
}

#[test]
fn a_daemon_of_another_build_is_kept_while_it_keeps_sessions_moved_away() {
    let model = Model::start(vec![
        Reply::call("bash", json!({"command": WAIT})),
        Reply::done("Finished."),
    ]);
    let mut older = Term::spawn(&model, &["-y"], &[("LLX_BUILD", "0.0.0+older")]);
    older.wait_for("❯", LONG);
    backgrounded(&mut older);
    // The task ends with nobody attached: the daemon works on nothing, and keeps the session.
    std::fs::write(older.file("go"), "").unwrap();
    let said = status_with(&older, "0 working");
    assert!(
        said.contains("0 attached") && said.contains("0 working"),
        "{said}"
    );
    let newer = older.beside(&model, &[]);
    let said = String::from_utf8_lossy(&newer.raw()).into_owned();
    assert!(said.contains("this session runs in this process"), "{said}");
    assert!(status(&newer).contains("build 0.0.0+older"));
}

#[test]
fn a_task_on_the_command_line_is_run_where_sessions_wait_in_the_background() {
    let model = Model::start(vec![
        Reply::call("bash", json!({"command": WAIT})),
        Reply::done("Fresh."),
    ]);
    let mut first = Term::start(&model, &["-y"]);
    backgrounded(&mut first);
    let mut second = first.next_to(&model, &["a fresh task"]);
    let screen = second.wait_for("Fresh.", LONG);
    assert!(!screen.contains("a long task"), "{screen}");
    assert!(said(&model, 1).contains("a fresh task"));
    // The sessions moved away wait for a plain `llx` here.
    second.press(key::CTRL_D);
    assert!(second.exited(LONG).is_some(), "{}", second.screen());
    let third = first.beside(&model, &[]);
    third.wait_for("❯ a long task", LONG);
}

#[test]
fn a_session_open_at_one_terminal_is_not_opened_at_another() {
    let model = Model::start(vec![]);
    let turns = r#"[{"goal": "fix the parser", "answer": "Fixed.", "status": "done", "at": 1}]"#;
    let first = Term::resuming(&model, "1700000000-ab12", turns);
    let mut second = first.next_to(&model, &["resume", "1700000000-ab12"]);
    let screen = second.wait_for("open at another terminal", LONG);
    assert!(screen.contains("1700000000-ab12"), "{screen}");
    assert!(second.exited(LONG).is_some(), "{}", second.screen());
    first.wait_for("❯ fix the parser", LONG);
}
