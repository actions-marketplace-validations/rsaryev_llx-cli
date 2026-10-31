use contracts::chat::Spend;

use super::*;

fn bash_call() -> ToolCall {
    ToolCall {
        id: "c1".into(),
        name: "bash".into(),
        arguments: json!({"command": "ls"}),
    }
}

#[test]
fn a_call_and_its_result_share_an_id() {
    let call_event = call(&bash_call(), 5);
    assert_eq!(call_event["type"], "tool.call");
    assert_eq!(call_event["arguments"]["command"], "ls");
    let done = Ok(ToolOutput::from("exit 0".to_string()));
    let result_event = result(&bash_call(), &done, 9);
    assert_eq!(result_event["type"], "tool.result");
    assert_eq!(result_event["id"], call_event["id"]);
    assert_eq!(result_event["failed"], false);
    assert_eq!(result_event["output"], "exit 0");
    let fatal = Err(ToolError::Fatal("gone".into()));
    let failed = result(&bash_call(), &fatal, 9);
    assert_eq!(
        (failed["failed"].clone(), failed["fatal"].clone()),
        (json!(true), json!(true))
    );
    assert_eq!(failed["output"], "gone");
}

#[test]
fn the_run_completes_with_its_status_usage_and_data() {
    let agent = Spend {
        calls: 2,
        cost: 0.5,
        millis: 1200,
        ..Spend::default()
    };
    let data = Ok(json!({"n": 1}));
    let summary = Summary {
        status: "done",
        code: 0,
        answer: "Done.",
        data: Some(&data),
        error: None,
        agent: &agent,
        millis: 42,
        session: "1-ab",
    };
    let event = completed(&summary);
    assert_eq!(event["type"], "run.completed");
    assert_eq!(event["status"], "done");
    assert_eq!(event["exit_code"], 0);
    assert_eq!(event["data"], json!({"n": 1}));
    // Scripts and the Action read these: a rename must fail here, not report zeros there.
    assert_eq!(event["usage"]["agent"]["calls"], 2);
    assert_eq!(event["usage"]["agent"]["ms"], 1200);
    assert_eq!(event["usage"]["cost_usd"], 0.5);
    assert_eq!(event["duration_ms"], 42);
    assert!(event["error"].is_null());
}

#[test]
fn the_run_starts_with_the_schema_version() {
    let start = Start {
        goal: "fix it",
        session: "1-ab",
        model: "m",
        workspace: "/w",
    };
    let event = started(&start);
    assert_eq!(event["type"], "run.started");
    assert_eq!(event["schema"], SCHEMA);
    assert_eq!(event["workspace"], "/w");
}
