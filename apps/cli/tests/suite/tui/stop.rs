//! Stopping the task the session shows: its own stop, known by the session's id.

use serde_json::json;

use super::LONG;
use crate::common::model::{Model, Reply};
use crate::common::term::{Term, key};

#[test]
fn esc_stops_the_task_running() {
    // The command would run for 10 s: only the stop ends it sooner.
    let command = "for i in $(seq 200); do [ -e go ] && break; sleep 0.05; done";
    let model = Model::start(vec![
        Reply::call("bash", json!({"command": command})),
        Reply::done("Done."),
    ]);
    let mut term = Term::start(&model, &["-y"]);
    term.type_text("wait a little").press(key::ENTER);
    term.wait_for("Running for i in", LONG);
    term.press(key::ESC);
    term.wait_for("Stopped after", LONG);
}
