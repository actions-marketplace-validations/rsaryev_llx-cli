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

#[test]
fn login_adds_a_host_by_its_address_and_takes_its_model() {
    let model = Model::start(vec![Reply::done("Hi.")]);
    let mut term = Term::start(&model, &[]);
    term.type_text("/login").press(key::ENTER);
    term.wait_for("Search: ▏", LONG);
    term.type_text("another");
    term.wait_for("❯ Another host", LONG);
    term.press(key::ENTER);
    // Its address, then what to call it (Enter keeps the name offered), then its key (none).
    term.wait_for("Login › Another host › Address", LONG);
    term.type_text(&model.base).press(key::ENTER);
    term.wait_for("› Name", LONG);
    // A provider's own name is asked again at once, not after the key.
    term.type_text("deepseek").press(key::ENTER);
    term.wait_for("deepseek is a provider's own name; another", LONG);
    term.type_text("work").press(key::ENTER);
    term.wait_for("Login › work › API key", LONG);
    term.press(key::ENTER);
    // It answered: its models are the next step, and the one taken is the session's.
    let screen = term.wait_for("Login › work › Model", LONG);
    assert!(screen.contains("local-model"), "{screen}");
    term.press(key::ENTER);
    term.wait_for("Now work · local-model", LONG);
    // Saved as a server on this machine, with nothing else written beside the choice.
    let saved = std::fs::read_to_string(term.home().join("config/llx/settings.json")).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        saved["local"],
        serde_json::json!([{"name": "work", "url": model.base}])
    );
    assert!(saved.get("hosts").is_none(), "{saved}");
}

#[test]
fn model_lists_a_server_on_this_machine_under_local_and_takes_its_model() {
    let model = Model::start(vec![Reply::done("Hi.")]);
    let mut term = Term::with_local(&model);
    term.type_text("/model").press(key::ENTER);
    let screen = term.wait_for("Local: box 1", LONG);
    assert!(screen.contains("Cloud: [OpenRouter"), "{screen}");
    // Shift+Tab from the first tab goes round to the last: the local host's.
    term.press(key::BACK_TAB);
    let screen = term.wait_for("Local: [box 1]", LONG);
    assert!(screen.contains("❯ local-model"), "{screen}");
    term.press(key::ENTER);
    term.wait_for("Now box · local-model", LONG);
}

#[test]
fn model_lists_a_host_the_settings_name_under_hosts() {
    let model = Model::start(vec![Reply::done("Hi.")]);
    let mut term = Term::with_host(&model);
    term.type_text("/model").press(key::ENTER);
    let screen = term.wait_for("Hosts: box 1", LONG);
    assert!(screen.contains("Cloud: [OpenRouter"), "{screen}");
    term.press(key::BACK_TAB);
    term.wait_for("Hosts: [box 1]", LONG);
    term.press(key::ENTER);
    term.wait_for("Now box · local-model", LONG);
}
