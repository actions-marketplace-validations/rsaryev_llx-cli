//! The answer as the interface draws it, and what the model is told so that it writes one the
//! interface can draw.

use super::LONG;
use crate::common::home::Home;
use crate::common::model::{Model, Reply};
use crate::common::term::{Term, key};

#[test]
fn a_session_tells_the_model_the_forms_and_draws_the_one_it_writes() {
    let answer = "> [!STEPS] Cut a release\n1. **Run the checks.**\n2. **Tag it.**\n";
    let model = Model::start(vec![Reply::done(answer)]);
    let mut term = Term::start(&model, &[]);
    term.type_text("how do I release").press(key::ENTER);
    let screen = term.wait_for("Tag it.", LONG);
    // Steps, not the quote and the list they were written as.
    assert!(screen.contains("Cut a release"), "{screen}");
    assert!(!screen.contains("[!STEPS]"), "{screen}");
    let sent = model.requests()[0].to_string();
    assert!(sent.contains("# Answer format"), "{sent}");
    assert!(sent.contains("> [!STEPS]") && sent.contains("> [!SOURCES]"));
}

#[test]
fn an_answer_nothing_draws_is_asked_for_as_it_is() {
    // Through a pipe the answer is printed as written: a form there would be noise.
    let home = Home::new();
    let model = Model::start(vec![Reply::done("Done.")]);
    let out = home.exec(&model, &["exec", "x"], "");
    assert_eq!(out.status.code(), Some(0));
    let sent = model.requests()[0].to_string();
    assert!(sent.contains("<environment>"), "{sent}");
    // The system prompt names the block; the context holds none.
    assert!(!sent.contains("# Answer format") && !sent.contains("[!STEPS]"));
}
