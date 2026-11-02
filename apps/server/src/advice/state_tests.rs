use contracts::advice::Task;
use contracts::chat::{Message, ToolCall};
use serde_json::{Value, json};

use super::*;

fn call(id: &str, name: &str, arguments: Value) -> Message {
    Message::Assistant {
        text: None,
        calls: vec![ToolCall {
            id: id.into(),
            name: name.into(),
            arguments,
        }],
        carried: vec![],
    }
}

fn result(id: &str, content: &str, failed: bool) -> Message {
    Message::ToolResult {
        id: id.into(),
        content: content.into(),
        failed,
    }
}

fn task(conversation: &[Message], from: usize) -> Task<'_> {
    Task {
        goal: "Make it fast",
        conversation,
        from,
    }
}

#[test]
fn a_task_reads_as_its_goal_its_calls_with_what_they_came_to_and_its_answer() {
    let conversation = [
        call("old", "bash", json!({"command": "ls"})),
        result("old", "earlier", false),
        Message::User("Goal: Make it fast".into()),
        call("t", "bash", json!({"command": "cargo test"})),
        result("t", "test result: ok", false),
        call(
            "p",
            "apply_patch",
            json!("*** Begin Patch\n*** Update File: a.rs\n@@\n-x\n+y\n*** End Patch"),
        ),
        result("p", "Success. Updated the following files:\nM a.rs", false),
    ];
    let seen = review(&task(&conversation, 2), "All done.");
    let state = &seen.state;
    assert_eq!(
        (state["goal"].as_str(), state["answer"].as_str()),
        (Some("Make it fast"), Some("All done."))
    );
    let steps = state["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2, "the calls before the task are not its own");
    assert_eq!(steps[0]["call"], "bash");
    assert_eq!(steps[0]["input"], "cargo test");
    assert_eq!(steps[0]["output"], "test result: ok");
    assert_eq!(
        steps[1]["input"], "*** Update File: a.rs",
        "a patch reads as its files"
    );
    assert!(seen.changed);
    assert_eq!(state["facts"]["checks_run_after_the_last_change"], false);
}

#[test]
fn a_long_task_keeps_its_latest_steps_within_what_the_decision_model_reads() {
    let mut conversation = vec![Message::User("Goal: Make it fast".into())];
    for at in 0..400 {
        let id = format!("c{at}");
        conversation.push(call(&id, "bash", json!({"command": format!("step {at}")})));
        conversation.push(result(&id, &"x".repeat(5_000), at % 2 == 0));
    }
    let seen = review(&task(&conversation, 0), &"y".repeat(50_000));
    let size = seen.state.to_string().chars().count();
    assert!(size <= MAX_STATE, "{size}");
    let steps = seen.state["steps"].as_array().unwrap();
    assert_eq!(steps.last().unwrap()["input"], "step 399");
    assert!(
        seen.state["facts"]["earlier_steps_left_out"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(!seen.changed);
}

#[test]
fn the_state_names_the_mode_the_task_works_in_the_latest_note_first() {
    let context = "<context>\n<environment>\nWorking directory: /w\nMode: plan: Read-only: a plan \
        achieves the goal.\nDate: 2026-10-31\n</environment>\n</context>";
    let mut conversation = vec![
        Message::System("rules".into()),
        Message::User(context.into()),
        Message::User("Goal: Add a flag".into()),
    ];
    let seen = review(&task(&conversation, 2), "The plan: …");
    assert_eq!(
        seen.state["mode"],
        "plan: Read-only: a plan achieves the goal."
    );
    conversation.insert(
        2,
        Message::User(
            "<mode_change>\nThe mode is now default: Works in the workspace.\nWhat the context or \
         an earlier note said about the mode no longer holds.\n</mode_change>"
                .into(),
        ),
    );
    let seen = review(&task(&conversation, 3), "Done.");
    assert_eq!(seen.state["mode"], "default: Works in the workspace.");
}

#[test]
fn what_came_in_during_the_task_reads_in_its_place_among_the_steps() {
    // A command's end, a monitor's lines or an agent's answer reach the task as a message, not as
    // a call's result: the answer that reports it is judged knowing it.
    let came = "<background job=\"ticks\" ended=\"Exit code: 0\">\nExit code: 0\nOutput:\ntick 1\n\
                </background>";
    let conversation = [
        Message::User("Goal: Make it fast".into()),
        call(
            "b",
            "bash",
            json!({"command": "for i in 1; do echo tick $i; done"}),
        ),
        result("b", "Running in the background: job `ticks` (job-1)", false),
        Message::User("<mode_change>The mode is now plan</mode_change>".into()),
        Message::User(came.into()),
    ];
    let seen = review(&task(&conversation, 0), "Exit code 0: tick 1.");
    let steps = seen.state["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2, "{steps:?}");
    assert_eq!(steps[0]["call"], "bash");
    assert_eq!(steps[1]["came_in"], came);
}
