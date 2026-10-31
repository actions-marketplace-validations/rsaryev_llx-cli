//! The decision model's advice at a task's edges (C9), as the chat model receives it: nothing
//! changes with none (no decision, `--no-advice`), a level picked under `auto` goes out with the
//! conversation's every request, and an answer found unfinished goes on.

use serde_json::{Value, json};

use crate::common::home::{Home, text};
use crate::common::model::{Model, Reply};

/// The chat requests alone: a scripted decision is kept among the requests too.
fn chats(model: &Model) -> Vec<Value> {
    let requests = model.requests();
    requests
        .into_iter()
        .filter(|request| request.get("messages").is_some())
        .collect()
}

#[test]
fn with_no_advice_the_model_receives_what_it_would_without_a_decision_model() {
    let script = || {
        vec![
            Reply::call("bash", json!({"command": "echo hi"})),
            Reply::done("Done."),
        ]
    };
    // One home: the context names its directory.
    let (home, quiet) = (Home::new(), Model::start(script()));
    let out = home.exec(&quiet, &["exec", "-y", "x"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let off = Model::start(script());
    let out = home.exec(&off, &["exec", "-y", "--no-advice", "x"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(chats(&quiet), chats(&off));
    assert!(!quiet.advised().is_empty(), "the decision model was asked");
    assert!(off.advised().is_empty(), "--no-advice asks nothing");
}

#[test]
fn a_level_picked_under_auto_goes_out_with_every_request_of_the_conversation() {
    let home = Home::new();
    let model = Model::start(vec![
        Reply::decided("low", &[("low", 0.9), ("medium", 0.1)]),
        Reply::call("bash", json!({"command": "echo hi"})),
        Reply::done("Done."),
    ]);
    let out = home.exec(&model, &["exec", "-y", "x"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let levels: Vec<Value> = chats(&model)
        .iter()
        .map(|r| r["reasoning"].clone())
        .collect();
    assert_eq!(levels, [json!({"effort": "low"}), json!({"effort": "low"})]);
    // A level the person names is theirs: nothing is asked as the task starts, so its first
    // request waits for no decision; only the answer is looked at.
    let home = Home::new();
    let model = Model::start(vec![
        Reply::done("Done."),
        Reply::decided("done", &[("done", 1.0)]),
    ]);
    let out = home.exec(&model, &["exec", "--reasoning", "high", "x"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(chats(&model)[0]["reasoning"], json!({"effort": "high"}));
    let asked: Vec<Value> = (model.advised().iter())
        .map(|decision| {
            json!(
                decision["questions"]
                    .as_object()
                    .map(|q| q.keys().collect::<Vec<_>>())
            )
        })
        .collect();
    assert_eq!(asked, [json!(["verdict"])]);
}

#[test]
fn an_answer_found_unfinished_goes_on_with_the_review() {
    let home = Home::new();
    let model = Model::start(vec![
        Reply::done("Half of it is done; the rest is left."),
        Reply::decided(
            "unfinished",
            &[("unfinished", 0.9), ("done", 0.05), ("blocked", 0.05)],
        ),
        Reply::done("All of it is done."),
    ]);
    let out = home.exec(&model, &["exec", "x"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout).trim(), "All of it is done.");
    let chats = chats(&model);
    assert_eq!(chats.len(), 2);
    let told = chats[1]["messages"].to_string();
    assert!(told.contains("A review of your answer"), "{told}");
}

#[test]
fn what_the_advice_costs_counts_toward_the_budget() {
    let home = Home::new();
    // A completion costs 0.0001 and a decision 0.00002: the review takes the task past 0.00011,
    // so it does not go on.
    let model = Model::start(vec![
        Reply::done("Half of it is done; the rest is left."),
        Reply::decided("unfinished", &[("unfinished", 1.0)]),
        Reply::done("All of it is done."),
    ]);
    let out = home.exec(&model, &["exec", "--budget", "0.00011", "x"], "");
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("the budget ran out"), "{stderr}");
    assert_eq!(chats(&model).len(), 1);
}
