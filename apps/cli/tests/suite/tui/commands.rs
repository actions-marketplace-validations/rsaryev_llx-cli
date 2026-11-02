//! The slash commands at the prompt.

use super::{LONG, until_file};
use crate::common::model::{Model, Reply};
use crate::common::term::{Term, key};

#[test]
fn slash_down_enter_opens_model_which_applies_the_one_chosen() {
    // The bug a person hit: "/" alone, Down to a command, Enter said "unknown command /".
    // `/model` is the second command offered: Down marks it, Enter opens its picker.
    let model = Model::start(vec![Reply::done("Hi.")]);
    let mut term = Term::start(&model, &[]);
    term.press("/");
    term.wait_for("/model", LONG);
    term.press(key::DOWN).press(key::ENTER);
    let screen = term.wait_for("• Model", LONG);
    assert!(!screen.contains("unknown command"), "{screen}");
    assert!(screen.contains("OpenRouter"), "{screen}");
    assert!(screen.contains("❯"), "a model is marked:\n{screen}");
    // Off the current model, so a /model that applied nothing would show.
    term.press(key::DOWN).press(key::ENTER);
    // One Enter confirms both the model and its inline reasoning level.
    term.wait_for("Now openrouter", LONG);
    term.type_text("hello").press(key::ENTER);
    term.wait_for("Worked for", LONG);
    let used = model.requests()[0]["model"].as_str().unwrap().to_string();
    assert_ne!(used, "anthropic/claude-haiku-5.5");
    let settings = std::fs::read_to_string(term.file("../config/llx/settings.json")).unwrap();
    assert!(settings.contains(&used), "{used} is not in {settings}");
}

#[test]
fn the_header_names_the_model_model_switched_to() {
    // The bug a person saw: after /model the header line still named the model the session
    // started with. A switch prints the header again, with the model now in use.
    let model = Model::start(vec![]);
    let mut term = Term::start(&model, &[]);
    term.type_text("/model").press(key::ENTER);
    term.wait_for("• Model", LONG);
    term.press(key::DOWN).press(key::ENTER);
    let screen = term.wait_for("Now openrouter · ", LONG);
    let chosen = screen
        .lines()
        .find_map(|line| line.trim().strip_prefix("Now openrouter · "))
        .and_then(|rest| rest.split(' ').next())
        .unwrap()
        .to_string();
    term.wait_for(&format!("· openrouter · {chosen} ·"), LONG);
}

#[test]
fn slash_mode_offers_the_modes_the_server_says() {
    let model = Model::start(vec![]);
    let mut term = Term::start(&model, &[]);
    term.type_text("/mode by");
    let screen = term.wait_for("bypass", LONG);
    assert!(
        screen.contains("Everything runs without asking"),
        "{screen}"
    );
}

#[test]
fn model_offers_a_provider_keyed_only_in_this_terminal() {
    // The bug: in the daemon, `/model` read the daemon's own environment (that of the terminal
    // that started it), so a key set only in this terminal's shell left its provider out.
    let model = Model::start(vec![]);
    let first = Term::start(&model, &[]);
    let key_here = [("ANTHROPIC_API_KEY", "sk-ant-test-0123456789")];
    let mut term = first.beside_with(&model, &[], &key_here);
    term.type_text("/model").press(key::ENTER);
    let screen = term.wait_for("• Model", LONG);
    assert!(screen.contains("Anthropic"), "{screen}");
}

#[test]
fn a_command_given_as_the_first_message_is_carried_out_not_sent_to_the_model() {
    // The bug: `llx "/status"` went to the model as a task.
    let model = Model::start(vec![]);
    let term = Term::start(&model, &["/status"]);
    let screen = term.wait_for("workspace  ", LONG);
    assert!(screen.contains("session    "), "{screen}");
    assert!(model.requests().is_empty(), "{:?}", model.requests());
}

#[test]
fn a_new_kept_for_after_the_task_says_how_to_open_a_session() {
    // The bug: `/clear` (another spelling of `/new`) kept with Tab for after the task reached
    // the session, which did nothing with it and said nothing.
    let model = Model::start(vec![
        Reply::call("bash", serde_json::json!({"command": until_file("go")})),
        Reply::done("Done."),
    ]);
    let mut term = Term::start(&model, &["-y"]);
    term.type_text("wait a little").press(key::ENTER);
    term.wait_for("Running for i in", LONG);
    term.type_text("/clear").press(key::TAB);
    term.wait_for("⏎ /clear", LONG);
    std::fs::write(term.file("go"), "").unwrap();
    term.wait_for("/new opens a session when typed at the prompt", LONG);
}

#[test]
fn a_mistyped_command_or_mode_says_what_there_is_and_asks_the_model_nothing() {
    let model = Model::start(vec![]);
    let mut term = Term::start(&model, &[]);
    term.type_text("/frobnicate").press(key::ENTER);
    term.wait_for("unknown command /frobnicate: /help lists them", LONG);
    term.type_text("/mode planning").press(key::ENTER);
    term.wait_for("unknown mode planning: default, plan, bypass", LONG);
    // Still in the mode it was, not the nearest name.
    term.type_text("/status").press(key::ENTER);
    let screen = term.wait_for("workspace  ", LONG);
    assert!(screen.contains("mode       default"), "{screen}");
    assert!(model.requests().is_empty(), "{:?}", model.requests());
}
