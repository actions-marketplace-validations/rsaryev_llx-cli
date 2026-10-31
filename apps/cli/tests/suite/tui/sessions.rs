//! Sessions side by side: ← lists them, a new one starts while another works, a session in the
//! background asks and is answered, Ctrl-C stops only the one shown, and quitting with work
//! running asks first.

use std::time::Duration;

use serde_json::json;

use super::LONG;
use crate::common::eventually;
use crate::common::model::{Model, Reply, said};
use crate::common::term::{Term, key};

/// A command that runs until the file `name` exists in the workspace (at most 10 s).
fn until_file(name: &str) -> String {
    format!("for i in $(seq 200); do [ -e {name} ] && break; sleep 0.05; done")
}

/// Wait until the screen no longer shows `needle`: another session is shown.
fn wait_gone(term: &Term, needle: &str) -> String {
    let gone = || Some(term.screen()).filter(|screen| !screen.contains(needle));
    eventually(LONG, Duration::from_millis(50), gone)
        .unwrap_or_else(|| panic!("{needle:?} stayed:\n{}", term.screen()))
}

/// ←, then "+ New session" (where the list starts with one session open): a new one, shown.
fn new_session(term: &mut Term, shown: &str) {
    term.press(key::LEFT);
    term.wait_for("Sessions in this project", LONG);
    term.wait_for("❯ + New session", LONG);
    term.press(key::ENTER);
    wait_gone(term, shown);
    term.wait_for("❯ Type a task", LONG);
}

#[test]
fn switch_away_while_a_task_runs() {
    let model = Model::start(vec![
        Reply::call("bash", json!({"command": until_file("go")})),
        Reply::done("B is done."),
        Reply::done("A is done."),
    ]);
    let mut term = Term::start(&model, &["-y"]);
    term.type_text("first task").press(key::ENTER);
    term.wait_for("Running for i in", LONG);
    new_session(&mut term, "❯ first task");
    term.type_text("second task").press(key::ENTER);
    term.wait_for("B is done.", LONG);
    term.wait_for("Worked for", LONG);
    // The first goes on in the background, and says so when it ends.
    term.press(key::LEFT);
    term.wait_for("running", LONG);
    term.press(key::ESC);
    std::fs::write(term.file("go"), "").unwrap();
    term.wait_for("Session \"first task\" finished", LONG);
    term.press(key::LEFT);
    let list = term.wait_for("unseen", LONG);
    assert!(list.contains("❯ ✓ first task"), "{list}");
    term.press(key::ENTER);
    let screen = wait_gone(&term, "❯ second task");
    assert!(screen.contains("❯ first task"), "{screen}");
    term.wait_for("A is done.", LONG);
    assert!(
        said(&model, 2).contains("first task"),
        "A's own conversation"
    );
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "a pause before the yes: keys typed as the question comes up must answer nothing"
)]
fn approval_from_background_session_is_labelled() {
    let gate = std::env::temp_dir().join(format!("llx-gate-{}", std::process::id()));
    let _ = std::fs::remove_file(&gate);
    let model = Model::start(vec![
        Reply::After(
            gate.clone(),
            Box::new(Reply::call(
                "bash",
                json!({"command": "echo from-a", "escalate": true}),
            )),
        ),
        Reply::done("A ran it."),
    ]);
    let mut term = Term::start(&model, &[]);
    term.type_text("first task").press(key::ENTER);
    term.wait_for("Working", LONG);
    new_session(&mut term, "❯ first task");
    std::fs::write(&gate, "").unwrap();
    let screen = term.wait_for("Run this command?", LONG);
    assert!(
        screen.contains("session first task (in the background)"),
        "{screen}"
    );
    assert!(screen.contains("echo from-a"), "{screen}");
    // Keys typed as it comes up answer nothing; then a yes.
    std::thread::sleep(Duration::from_millis(400));
    term.press("1");
    term.wait_for("Session \"first task\" finished", LONG);
    assert!(said(&model, 1).contains("from-a"), "the command ran");
    let _ = std::fs::remove_file(&gate);
}

#[test]
fn stop_shown_only() {
    let model = Model::start(vec![
        Reply::call("bash", json!({"command": until_file("go")})),
        Reply::call("bash", json!({"command": until_file("never")})),
        Reply::done("A is done."),
    ]);
    let mut term = Term::start(&model, &["-y"]);
    term.type_text("first task").press(key::ENTER);
    term.wait_for("Running for i in", LONG);
    new_session(&mut term, "❯ first task");
    term.type_text("second task").press(key::ENTER);
    term.wait_for("Running for i in", LONG);
    term.press(key::CTRL_C);
    term.wait_for("Stopped after", LONG);
    std::fs::write(term.file("go"), "").unwrap();
    term.wait_for("Session \"first task\" finished", LONG);
    let screen = term.screen();
    assert!(!screen.contains("first task\" was stopped"), "{screen}");
    assert!(said(&model, 2).contains("first task"), "A went on");
}

#[test]
fn quit_with_work_running_asks() {
    let command = format!("echo $$ > pid; {}", until_file("never"));
    let model = Model::start(vec![Reply::call("bash", json!({"command": command}))]);
    // In process: the work ends with the interface, so there is no background to offer.
    let mut term = Term::start(&model, &["-y", "--no-daemon"]);
    term.type_text("first task").press(key::ENTER);
    term.wait_for("Running echo", LONG);
    new_session(&mut term, "❯ first task");
    term.press(key::CTRL_D);
    let screen = term.wait_for("Background work is running", LONG);
    for said in [
        "will stop when you exit",
        "session · ",
        "1. Exit and stop tasks",
        "2. Stay",
    ] {
        assert!(screen.contains(said), "{said}: {screen}");
    }
    assert!(
        !screen.contains("Move to background"),
        "in process: {screen}"
    );
    // Stay: nothing stops.
    term.type_text("2");
    wait_gone(&term, "Background work is running");
    assert_eq!(term.exited(Duration::from_millis(300)), None);
    term.press(key::LEFT);
    term.wait_for("running", LONG);
    term.press(key::ESC);
    term.press(key::CTRL_D);
    term.wait_for("Exit and stop tasks", LONG);
    term.type_text("1");
    assert!(term.exited(Duration::from_secs(3)).is_some(), "it quit");
    let pid: i32 = std::fs::read_to_string(term.file("pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // SAFETY: signal 0 only asks whether the process exists.
    let alive = unsafe { libc::kill(pid, 0) } == 0;
    assert!(!alive, "the shell was stopped");
}

#[test]
fn a_session_is_titled_in_the_background_and_listed_and_saved_so() {
    let model = Model::titling(vec![Reply::done("Fixed.")], "Fix the parser");
    let mut term = Term::start(&model, &["-y"]);
    term.type_text("the parser drops the last token")
        .press(key::ENTER);
    term.wait_for("Fixed.", LONG);
    term.wait_for("Worked for", LONG);
    term.press(key::LEFT);
    let list = term.wait_for("Fix the parser", LONG);
    assert!(list.contains("✓ Fix the parser"), "{list}");
    assert_eq!(model.titled(), ["the parser drops the last token"]);
    assert_eq!(model.requests().len(), 1, "the task's own request alone");
    let titled = || {
        let saved = term.saved();
        saved
            .iter()
            .any(|saved| saved["title"] == "Fix the parser")
            .then_some(())
    };
    let titled = eventually(LONG, Duration::from_millis(50), titled);
    assert!(titled.is_some(), "{:?}", term.saved());
}

#[test]
fn a_resumed_session_shows_its_earlier_tasks() {
    let model = Model::start(vec![]);
    let turns = r#"[
        {"goal": "fix the parser", "answer": "Fixed the **parser**.", "status": "done", "at": 1},
        {"goal": "and the lexer", "answer": "", "status": "interrupted", "at": 2}
    ]"#;
    let term = Term::resuming(&model, "1700000000-ab12", turns);
    let screen = term.wait_for("and the lexer", LONG);
    for shown in [
        "2 earlier tasks",
        "❯ fix the parser",
        "• Fixed the parser.",
        "■ stopped",
    ] {
        assert!(screen.contains(shown), "{shown:?}:\n{screen}");
    }
    assert!(model.requests().is_empty(), "nothing is asked of the model");
}

#[test]
fn a_session_resumed_from_another_directory_says_so_on_the_screen() {
    // Served by the daemon, a note on stderr would go to its log, not to the person.
    let model = Model::start(vec![]);
    let turns = r#"[{"goal": "fix the parser", "answer": "Fixed.", "status": "done", "at": 1}]"#;
    let term = Term::resuming_from(&model, "1700000000-ef56", turns, Some("/elsewhere/repo"));
    let screen = term.wait_for("ran in /elsewhere/repo", LONG);
    assert!(screen.contains("session 1700000000-ef56"), "{screen}");
    // Said once, by the client: not again in the daemon's log.
    let log = term.daemon_log();
    assert!(!log.contains("ran in"), "{log}");
}

#[test]
fn a_resumed_session_shows_what_it_spent_before() {
    let model = Model::start(vec![]);
    let turns = r#"[
        {"goal": "fix the parser", "answer": "Fixed.", "status": "done", "at": 1, "steps": [
            {"what": "model", "at_ms": 0, "ms": 900, "spent": {"input_tokens": 20000,
                "cached_tokens": 21000, "output_tokens": 1000, "cost": 0.04}},
            {"what": "bash", "at_ms": 900, "ms": 100},
            {"what": "decision", "at_ms": 0, "ms": 0, "spent": {"input_tokens": 4100,
                "cached_tokens": 0, "output_tokens": 200, "cost": 0.002}}
        ]}
    ]"#;
    let term = Term::resuming(&model, "1700000000-cd34", turns);
    let screen = term.wait_for("1.2k out", LONG);
    assert!(
        screen.contains("24.1k in · 87% cached · 1.2k out · $0.042"),
        "{screen}"
    );
}
