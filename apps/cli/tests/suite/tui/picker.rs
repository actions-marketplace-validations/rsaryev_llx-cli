//! A long list put to the person: typed letters search it.

use super::LONG;
use crate::common::model::{Model, Reply};
use crate::common::term::{Term, key};

#[test]
fn login_finds_a_provider_by_typing_and_enter_takes_it() {
    let model = Model::start(vec![Reply::done("Hi.")]);
    let mut term = Term::start(&model, &[]);
    term.type_text("/login").press(key::ENTER);
    term.wait_for("Search: ▏", LONG);
    term.type_text("deeps");
    let screen = term.wait_for("Search: deeps▏  1 match", LONG);
    assert!(screen.contains("❯ DeepSeek"), "{screen}");
    assert!(!screen.contains("OpenRouter"), "{screen}");
    term.press(key::ENTER);
    term.wait_for("Login › DeepSeek › API key", LONG);
}

#[test]
fn escape_in_login_goes_back_one_step_and_at_the_first_ends_it() {
    let model = Model::start(vec![Reply::done("Hi.")]);
    let mut term = Term::start(&model, &[]);
    term.type_text("/login").press(key::ENTER);
    term.wait_for("Login › Provider", LONG);
    term.type_text("deeps").press(key::ENTER);
    term.wait_for("Login › DeepSeek › API key", LONG);
    // The key line is the second step: Esc shows the providers again.
    term.press(key::ESC);
    let screen = term.wait_for("Login › Provider", LONG);
    assert!(!screen.contains("paste it"), "{screen}");
    // And Esc at the first step ends the flow.
    term.press(key::ESC);
    term.wait_for("Nothing changed.", LONG);
}
