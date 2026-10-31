//! A message sent while the task works reaches it at its next step.

use serde_json::json;

use super::LONG;
use crate::common::model::{Model, Reply, said};
use crate::common::term::{Term, key};

#[test]
fn a_message_sent_while_a_command_runs_reaches_the_task() {
    // The command runs until the test lets it end (at most 10 s), so the message is sent while
    // it runs however slowly the keys are typed.
    let command = "for i in $(seq 200); do [ -e go ] && break; sleep 0.05; done";
    let model = Model::start(vec![
        Reply::call("bash", json!({"command": command})),
        Reply::done("Noted."),
    ]);
    let mut term = Term::start(&model, &["-y"]);
    term.type_text("wait a little").press(key::ENTER);
    term.wait_for("Running for i in", LONG);
    term.type_text("also check the README").press(key::ENTER);
    term.wait_for("↳ also check the README", LONG);
    std::fs::write(term.file("go"), "").unwrap();
    term.wait_for("Worked for", LONG);
    let next = said(&model, 1);
    assert!(
        next.contains("also check the README"),
        "steered into the task:\n{next}"
    );
}
