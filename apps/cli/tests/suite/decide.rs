//! `llx decide` as a script runs it: one question about stdin to the decision model, the option
//! it picked alone on stdout, and an exit code a pipeline can branch on.

use serde_json::Value;

use crate::common::home::{Home, text};
use crate::common::model::{Model, Reply};

fn decided(choice: &str, odds: &[(&str, f64)]) -> Model {
    Model::start(vec![Reply::decided(choice, odds)])
}

/// The one question the decision model was asked, and the state it was asked about.
fn asked(model: &Model) -> (Value, Value) {
    let request = model.requests().into_iter().next().unwrap();
    let question = request["questions"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    (question, request["state"].clone())
}

#[test]
fn the_option_picked_goes_alone_to_stdout_from_a_yes_or_no_about_stdin() {
    let (home, model) = (Home::new(), decided("no", &[("yes", 0.1), ("no", 0.9)]));
    let out = home.exec(
        &model,
        &["decide", "Does this break the public API?"],
        "-fn old()\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "no\n");
    let (question, state) = asked(&model);
    assert!(
        question["instructions"]
            .to_string()
            .contains("Does this break the public API?")
    );
    let keys: Vec<&String> = question["criteria"].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["no", "yes"]);
    assert!(state.to_string().contains("-fn old()"), "{state}");
    let request = &model.requests()[0];
    assert_eq!(request["model"], "~typesafe/jev-latest");
}

#[test]
fn the_choices_are_the_options_and_a_described_one_tells_the_model_what_it_means() {
    let (home, model) = (Home::new(), decided("minor", &[("minor", 0.8)]));
    let args = [
        "decide",
        "What release is this?",
        "--choices",
        "patch,minor=a new feature,major",
    ];
    let out = home.exec(&model, &args, "+fn new()\n");
    assert_eq!(text(&out.stdout), "minor\n", "{}", text(&out.stderr));
    let (question, _) = asked(&model);
    let criteria = question["criteria"].as_object().unwrap();
    let keys: Vec<&String> = criteria.keys().collect();
    assert_eq!(keys, ["major", "minor", "patch"]);
    assert_eq!(criteria["minor"], "a new feature");
}

#[test]
fn expect_ends_with_1_when_the_model_picked_another_option() {
    let (home, model) = (Home::new(), decided("yes", &[("yes", 0.7), ("no", 0.3)]));
    let out = home.exec(
        &model,
        &["decide", "Is it risky?", "--expect", "no"],
        "diff\n",
    );
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "yes\n");
    let (home, model) = (Home::new(), decided("no", &[("no", 0.9)]));
    let out = home.exec(
        &model,
        &["decide", "Is it risky?", "--expect", "no"],
        "diff\n",
    );
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn json_carries_the_probabilities_the_model_and_the_cost() {
    let (home, model) = (Home::new(), decided("no", &[("yes", 0.25), ("no", 0.75)]));
    let out = home.exec(&model, &["decide", "Is it risky?", "--json"], "diff\n");
    let said: Value = serde_json::from_str(&text(&out.stdout)).unwrap();
    assert_eq!(said["choice"], "no");
    assert_eq!(said["probabilities"]["no"], 0.75);
    assert_eq!(said["model"], "~typesafe/jev-latest");
    assert!(said["cost_usd"].as_f64().is_some(), "{said}");
}

#[test]
fn a_bad_choice_list_or_no_openrouter_key_is_not_ready() {
    let (home, model) = (Home::new(), decided("a", &[]));
    let out = home.exec(&model, &["decide", "Which?", "--choices", "only"], "x\n");
    assert_eq!(out.status.code(), Some(3));
    assert!(text(&out.stderr).contains("two"), "{}", text(&out.stderr));
    let out = home.exec(&model, &["decide", "Which?", "--expect", "maybe"], "x\n");
    assert_eq!(out.status.code(), Some(3));
    let home = Home::new();
    let out = home.run(&["decide", "Is it risky?"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(
        text(&out.stderr).contains("OpenRouter"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn a_repeated_choice_keeps_the_commas_of_its_description() {
    let (home, model) = (Home::new(), decided("infra", &[("infra", 0.9)]));
    let args = [
        "decide",
        "What kind of failure is this?",
        "--choices",
        "test=a test failed",
        "--choices",
        "infra=the runner, network or a service was down",
    ];
    let out = home.exec(&model, &args, "503\n");
    assert_eq!(text(&out.stdout), "infra\n", "{}", text(&out.stderr));
    let (question, _) = asked(&model);
    let criteria = question["criteria"].as_object().unwrap();
    assert_eq!(criteria.len(), 2, "{criteria:?}");
    assert_eq!(
        criteria["infra"],
        "the runner, network or a service was down"
    );
}

#[test]
fn stdin_is_read_up_to_256_kib_as_exec_reads_it_and_the_model_is_told_it_was_cut() {
    let (home, model) = (Home::new(), decided("no", &[("no", 0.9)]));
    let input = format!("{}ZZZZ-the-tail-ZZZZ", "a".repeat(256 * 1024));
    let out = home.exec(&model, &["decide", "Is it risky?"], &input);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "no\n");
    let (_, state) = asked(&model);
    let state = state.to_string();
    assert!(
        state.contains("[stdin cut at 256 KiB: the rest was not read]"),
        "the model is told: {}",
        &state[state.len().saturating_sub(120)..]
    );
    assert!(!state.contains("ZZZZ"), "the tail was not read");
}
