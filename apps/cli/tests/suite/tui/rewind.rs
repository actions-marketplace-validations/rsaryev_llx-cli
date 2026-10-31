//! Esc Esc rewinds the conversation to before a task; the files stay as the task left them.

use serde_json::json;

use super::LONG;
use crate::common::model::{Model, Reply, said};
use crate::common::term::{Term, key};

#[test]
fn esc_esc_rewinds_the_conversation_and_leaves_the_files() {
    let model = Model::start(vec![
        Reply::call("bash", json!({"command": "echo hello > notes.txt"})),
        Reply::done("Created notes.txt."),
    ]);
    let mut term = Term::start(&model, &["-y"]);
    term.type_text("create notes.txt").press(key::ENTER);
    term.wait_for("Worked for", LONG);
    assert!(said(&model, 0).contains("Goal: create notes.txt"));
    term.press(key::ESC).press(key::ESC);
    term.wait_for("Rewind the conversation to before", LONG);
    term.press(key::ENTER);
    let screen = term.wait_for("Rewound", LONG);
    assert!(screen.contains("the files stay as they are"), "{screen}");
    assert_eq!(
        std::fs::read_to_string(term.file("notes.txt")).unwrap(),
        "hello\n"
    );
    // The task is back in the prompt: the composer's row, not the history's copy of it.
    term.wait_for_cursor_row("❯ create notes.txt", LONG);
}
