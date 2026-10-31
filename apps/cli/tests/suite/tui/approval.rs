//! A command that must leave the sandbox, asked about at the terminal, and run once the person
//! says yes.

use serde_json::json;

use super::LONG;
use crate::common::model::{Model, Reply, said};
use crate::common::term::{Term, key};

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "the test waits for what must not happen: no event says it did not"
)]
fn a_command_is_asked_about_and_runs_once_approved() {
    let model = Model::start(vec![
        Reply::call(
            "bash",
            json!({"command": "echo approved-and-ran", "escalate": true}),
        ),
        Reply::done("It ran."),
    ]);
    let mut term = Term::start(&model, &[]);
    term.type_text("run it").press(key::ENTER);
    let screen = term.wait_for("Run this command?", LONG);
    assert!(
        screen.contains("echo approved-and-ran") && screen.contains("outside the sandbox"),
        "{screen}"
    );
    // Words typed ahead as the box opens answer nothing ("a" once meant "don't ask again").
    term.type_text("also");
    // Long enough for an "a" to have approved it and the command to have run.
    std::thread::sleep(std::time::Duration::from_millis(500));
    let screen = term.screen();
    assert!(
        screen.contains("Run this command?") && !screen.contains("Ran echo"),
        "{screen}"
    );
    term.press("1");
    term.wait_for("Worked for", LONG);
    assert!(
        said(&model, 1).contains("approved-and-ran"),
        "the model read its output"
    );
}
