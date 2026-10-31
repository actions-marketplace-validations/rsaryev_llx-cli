//! An agent the main one started: chosen in the list under the composer, shown, stopped with
//! `x`, and let go by `/rewind`.

use serde_json::json;

use super::LONG;
use crate::common::model::{Model, Reply};
use crate::common::term::{Term, key};

#[test]
fn an_agent_is_shown_from_the_list_stopped_with_x_and_let_go_by_rewind() {
    // The main agent and its agent each run this until the test lets them end (at most 20 s):
    // the two replies are the same, so it does not matter which of them asks first.
    let command = "for i in $(seq 400); do [ -e go ] && break; sleep 0.05; done";
    let model = Model::start(vec![
        Reply::call(
            "agents",
            json!({"action": "spawn", "task": "wait for go", "name": "scout"}),
        ),
        Reply::call("bash", json!({"command": command})),
        Reply::call("bash", json!({"command": command})),
    ]);
    let mut term = Term::start(&model, &["-y"]);
    term.type_text("start scout").press(key::ENTER);
    term.wait_for("scout", LONG);
    // ↓ into the list (on main), ↓ to scout, Enter shows its view; ← goes back to main.
    term.press(key::DOWN).press(key::DOWN).press(key::ENTER);
    term.wait_for("◆ agent scout", LONG);
    term.press(key::LEFT);
    term.wait_for("start scout", LONG);
    term.press(key::DOWN).press(key::DOWN).press("x");
    term.wait_for("Agent \"scout\" was stopped", LONG);
    std::fs::write(term.file("go"), "").unwrap();
    term.wait_for("Worked for", LONG);
    term.wait_for("■ scout", LONG);
    // Esc gives the list's keys back to the composer; Esc Esc there rewinds.
    term.press(key::ESC).press(key::ESC).press(key::ESC);
    term.wait_for("Rewind the conversation to before", LONG);
    term.press(key::ENTER);
    let screen = term.wait_for("Rewound", LONG);
    let tree_rows = screen.lines().filter(|row| row.contains("■ scout")).count();
    assert_eq!(tree_rows, 0, "the agent's row is gone:\n{screen}");
}
