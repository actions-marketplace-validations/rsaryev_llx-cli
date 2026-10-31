//! Ctrl+Z gives the terminal back and stops llx, as vim and less do; the shell's `fg` continues
//! it, the prompt drawn again with the draft. Quitting says how to resume the session.

use super::LONG;
use crate::common::model::{Model, Reply};
use crate::common::term::{Term, key};

#[test]
fn ctrl_z_stops_llx_and_fg_draws_the_prompt_again_with_the_draft() {
    let model = Model::start(vec![Reply::done("The sky is blue.")]);
    // In a shell with job control: llx's own child would be an orphaned group, where a stop is
    // discarded.
    let mut term = Term::in_shell(&model);
    term.type_text("$LLX").press(key::ENTER);
    term.wait_for("❯", LONG);
    term.type_text("what colour is the sky").press(key::ENTER);
    term.wait_for("The sky is blue.", LONG);
    term.type_text("half a thought");
    term.press(key::CTRL_Z);
    term.wait_for_any(&["Stopped", "Suspended", "stopped"], LONG);
    // The shell has the terminal (its readline's modes, not llx's): the prompt is back.
    term.wait_for_cursor_row("$", LONG);
    term.type_text("fg").press(key::ENTER);
    term.wait_for_cursor_row("❯ half a thought", LONG);
    // The session is the same: the draft goes, a second Ctrl-C quits, and says how to resume it.
    term.press(key::CTRL_C).press(key::CTRL_C);
    let screen = term.wait_for("Resume this session: llx resume ", LONG);
    assert!(screen.contains("The sky is blue."), "{screen}");
}
