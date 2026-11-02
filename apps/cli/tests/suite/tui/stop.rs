//! Stopping the task the session shows: its own stop, known by the session's id.

use serde_json::json;

use super::{LONG, until_file};
use crate::common::model::{Model, Reply};
use crate::common::term::{Term, key};

#[test]
fn esc_stops_the_task_running() {
    // The command would run for 10 s: only the stop ends it sooner.
    let model = Model::start(vec![
        Reply::call("bash", json!({"command": until_file("go")})),
        Reply::done("Done."),
    ]);
    let mut term = Term::start(&model, &["-y"]);
    term.type_text("wait a little").press(key::ENTER);
    term.wait_for("Running for i in", LONG);
    term.press(key::ESC);
    term.wait_for("Stopped after", LONG);
}
