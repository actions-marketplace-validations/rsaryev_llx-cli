use agent::observer::Observer;
use contracts::chat::{ToolCall, Usage};
use contracts::tool::{ToolError, ToolOutput};
use serde_json::json;

use std::sync::Arc;

use super::*;
use crate::testing::{Captured, Closed};

/// A trace that keeps the events it is told, and draws each as its kind's name on a line.
fn recorder() -> (Drawn, Arc<Mutex<Vec<Event>>>) {
    let told = Arc::new(Mutex::new(vec![]));
    let kept = Arc::clone(&told);
    let trace = Box::new(move |event: &Event| {
        lock(&kept).push(event.clone());
        let kind = format!("{event:?}");
        let name = kind.split([' ', '(']).next().unwrap_or_default();
        format!("{name}\n")
    });
    let drawn = Drawn {
        trace,
        answer: None,
    };
    (drawn, told)
}

/// What a run of `bash` running `cargo test` tells of its call and its result.
fn told_call() -> [Event; 2] {
    let label = protocol::Label {
        doing: "Running".into(),
        done: "Ran".into(),
        subject: "cargo test".into(),
    };
    let call = Event::Call {
        call: "c1".into(),
        name: "bash".into(),
        label: label.clone(),
    };
    let report = "Exit code: 0\nWall time: 1.0 seconds\nOutput:".into();
    let outcome = protocol::Outcome::Done {
        report,
        shown: String::new(),
    };
    let result = Event::Result {
        call: "c1".into(),
        name: "bash".into(),
        label,
        outcome,
    };
    [call, result]
}

fn call() -> ToolCall {
    ToolCall {
        id: "c1".into(),
        name: "bash".into(),
        arguments: json!({"command": "cargo test"}),
    }
}

fn summary(agent: &Spend) -> Summary<'_> {
    Summary {
        status: "done",
        code: 0,
        answer: "All tests pass.",
        data: None,
        error: None,
        agent,
        millis: 10,
        session: "1-ab",
    }
}

fn run(output: &Output) {
    let start = Start {
        goal: "run the tests",
        session: "1-ab",
        model: "m",
        workspace: "/w",
    };
    output.started(&start);
    output.call(&call());
    let done = Ok(ToolOutput {
        report: "Exit code: 0\nWall time: 1.0 seconds\nOutput:".into(),
        ..ToolOutput::default()
    });
    output.result(&call(), &done);
    output.turn(&Usage::default());
    let agent = Spend::default();
    output.completed(&summary(&agent));
}

#[test]
fn json_is_one_event_per_line_on_stdout_and_nothing_on_stderr() {
    let (out, err) = (Captured::default(), Captured::default());
    let output = Output::with(true, false, Box::new(out.clone()), Box::new(err.clone()));
    run(&output);
    let types: Vec<String> = out
        .text()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap()["type"].to_string())
        .collect();
    assert_eq!(
        types,
        [
            "\"run.started\"",
            "\"tool.call\"",
            "\"tool.result\"",
            "\"turn\"",
            "\"run.completed\""
        ]
    );
    assert_eq!(err.text(), "");
}

#[test]
fn json_draws_no_trace_but_with_v() {
    for (verbose, drawn) in [(false, false), (true, true)] {
        let (out, err) = (Captured::default(), Captured::default());
        let (trace, told) = recorder();
        let output = Output::with(true, verbose, Box::new(out.clone()), Box::new(err.clone()));
        run(&output.drawn(Some(trace)));
        assert_eq!(!lock(&told).is_empty(), drawn, "-v: {verbose}");
        assert_eq!(err.text().contains("Result\n"), drawn, "{}", err.text());
        assert!(!out.text().contains("Result"), "{}", out.text());
    }
}

#[test]
fn a_person_reads_the_trace_on_stderr_and_the_answer_alone_on_stdout() {
    let (out, err) = (Captured::default(), Captured::default());
    let (trace, told) = recorder();
    let output = Output::with(false, false, Box::new(out.clone()), Box::new(err.clone()));
    run(&output.drawn(Some(trace)));
    assert_eq!(out.text(), "All tests pass.\n");
    // The trace is told what the interface is (`tui::trace` draws it), how the task ended last.
    let [call, result] = told_call();
    let end = Event::TaskEnded {
        status: "done".into(),
        millis: 10,
        turns: 0,
        cost: 0.0,
    };
    let spend = Event::Note("session  1-ab\n".into());
    assert_eq!(*lock(&told), [call, result, end, spend]);
    assert_eq!(err.text(), "Call\nResult\nTaskEnded\nNote\n");
}

#[test]
fn at_a_terminal_the_answer_is_drawn_on_stdout_unless_it_is_data() {
    let answer = |text: &str| format!("\n[{text}]\n");
    let drawn = || Drawn {
        trace: Box::new(|_: &Event| String::new()),
        answer: Some(Box::new(answer)),
    };
    let out = Captured::default();
    let output = Output::with(
        false,
        false,
        Box::new(out.clone()),
        Box::new(Captured::default()),
    );
    output
        .drawn(Some(drawn()))
        .completed(&summary(&Spend::default()));
    assert_eq!(out.text(), "\n[All tests pass.]\n");
    let out = Captured::default();
    let output = Output::with(
        false,
        false,
        Box::new(out.clone()),
        Box::new(Captured::default()),
    );
    let data = Ok(json!({"ok": true}));
    output.drawn(Some(drawn())).completed(&Summary {
        data: Some(&data),
        ..summary(&Spend::default())
    });
    assert_eq!(out.text(), "{\"ok\":true}\n");
}

#[test]
fn a_closed_stdout_is_noticed_and_never_panics() {
    let err = Captured::default();
    let output = Output::with(true, false, Box::new(Closed), Box::new(err));
    assert!(!output.closed());
    run(&output);
    assert!(output.closed());
}

#[test]
fn what_the_model_says_beside_its_calls_is_in_the_trace_and_not_in_the_events() {
    let (out, err) = (Captured::default(), Captured::default());
    let (trace, told) = recorder();
    let output = Output::with(false, false, Box::new(out.clone()), Box::new(err.clone()));
    let output = output.drawn(Some(trace));
    output.said("I will check the lockfile first.");
    output.call(&call());
    let said = Event::Answer("I will check the lockfile first.".into());
    assert_eq!(lock(&told)[0], said);
    assert_eq!(err.text(), "Answer\nCall\n");
    assert_eq!(out.text(), "", "stdout gets the answer alone");
    let (out, err) = (Captured::default(), Captured::default());
    let output = Output::with(true, false, Box::new(out.clone()), Box::new(err.clone()));
    output.said("I will check the lockfile first.");
    assert_eq!(
        (out.text(), err.text()),
        (String::new(), String::new()),
        "no event without a schema change"
    );
}

#[test]
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
fn a_turn_tells_the_interface_its_tokens() {
    let (ui, frames) = crate::testing::fake_ui();
    let output = crate::testing::quiet().in_ui(Some(ui.for_lane("lane")));
    let usage = Usage {
        input_tokens: 1000,
        output_tokens: 200,
        ..Default::default()
    };
    Observer::turn(&output, &usage);
    let events = crate::testing::events(&frames);
    let spent = events.into_iter().find_map(|(lane, event)| match event {
        Event::Spent(tokens) => Some((lane, tokens)),
        _ => None,
    });
    assert_eq!(spent, Some((Some("lane".to_string()), 1200)));
}

#[test]
fn a_task_keeps_what_it_did_each_call_and_turn_with_its_time() {
    let output = crate::testing::quiet();
    run(&output);
    // A task that just began: what came before was the last one's.
    output.take_steps();
    let call = call();
    output.call(&call);
    output.result(&call, &Err(ToolError::Failed("no".into())));
    output.turn(&Usage {
        millis: 1200,
        input_tokens: 900,
        output_tokens: 40,
        ..Usage::default()
    });
    let steps = output.take_steps();
    assert_eq!(steps.len(), 2, "{steps:?}");
    // The turn began 1.2 s before it ended, at the clock's start: before the call or with it.
    assert!(steps.is_sorted_by_key(|step| step.at_ms), "{steps:?}");
    let step = |what: &str| steps.iter().find(|step| step.what == what).unwrap();
    assert!(step("bash").failed, "{steps:?}");
    assert!(step("bash").detail.starts_with("Running"), "{steps:?}");
    assert_eq!(step("model").ms, 1200);
    assert_eq!(step("model").detail, "900 in · 40 out");
    assert!(output.take_steps().is_empty(), "taken once");
}

#[test]
fn on_a_github_runner_the_trace_cannot_issue_workflow_commands() {
    // A file or a model's answer can hold `::add-mask::` or `##[add-matcher]`: the runner reads
    // such a line on either stream as a command. The trace on stderr
    // is what is guarded here; stdout is pinned below to stay as it is.
    let err = Captured::default();
    let mut output = Output::with(
        false,
        true,
        Box::new(Captured::default()),
        Box::new(err.clone()),
    );
    let text = "::add-mask::abc\n  ::set-env name=X::1\n##[add-matcher]m.json\nok :: fine\n";
    output.runner = Runner::GitHub;
    output.note(text);
    for line in err.text().lines() {
        let line = line.trim_start();
        assert!(
            !line.starts_with("::") && !line.starts_with("##["),
            "{line:?}"
        );
    }
    assert!(err.text().contains("ok :: fine"));
    assert_eq!(
        err.text().replace('\u{200b}', ""),
        text,
        "what the person reads stays"
    );
}

#[test]
fn on_a_github_runner_the_answer_on_stdout_stays_byte_exact() {
    // Stdout is the answer and the data, which a pipe reads as they are: only the trace is guarded.
    let (out, err) = (Captured::default(), Captured::default());
    let mut output = Output::with(false, false, Box::new(out.clone()), Box::new(err));
    output.runner = Runner::GitHub;
    let agent = Spend::default();
    let answer = "::set-output name=x::1\n##[add-matcher]m.json";
    output.completed(&Summary {
        answer,
        ..summary(&agent)
    });
    assert_eq!(out.text(), format!("{answer}\n"));
}

/// The detail a task's step keeps of the call `name` with `arguments`, in the saved session.
fn detail(name: &str, arguments: Value) -> String {
    let output = crate::testing::quiet();
    output.call(&ToolCall {
        id: "c1".into(),
        name: name.into(),
        arguments,
    });
    output.take_steps().remove(0).detail
}

#[test]
fn a_step_keeps_what_its_call_does_cut_to_120_columns_as_the_session_saves_it() {
    assert_eq!(
        detail("bash", json!({"command": "cargo test", "workdir": "app"})),
        "Running cargo test (in app)"
    );
    // A call with no subject reads as its verb alone.
    assert_eq!(detail("bash", json!({})), "Running");
    // Columns, not characters: a wide character takes two.
    let wide = "汉".repeat(70);
    assert_eq!(
        detail("bash", json!({ "command": wide })),
        format!("Running {}…", "汉".repeat(55))
    );
    let long = "x".repeat(200);
    assert_eq!(
        detail("bash", json!({ "command": long })),
        format!("Running {}…", "x".repeat(111))
    );
}

#[test]
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
fn with_v_a_session_shows_each_turn_as_a_note() {
    let (ui, frames) = crate::testing::fake_ui();
    let (trace, told) = recorder();
    let output = Output::with(
        false,
        true,
        Box::new(Captured::default()),
        Box::new(Captured::default()),
    )
    .drawn(Some(trace))
    .in_ui(Some(ui.for_lane("lane")));
    let usage = Usage {
        model: "m/x".into(),
        input_tokens: 12_345,
        output_tokens: 67,
        cost: 0.0123,
        millis: 1500,
        ..Usage::default()
    };
    Observer::turn(&output, &usage);
    let notes: Vec<String> = (crate::testing::events(&frames).into_iter())
        .filter_map(|(_, event)| match event {
            Event::Note(text) => Some(text),
            _ => None,
        })
        .collect();
    // The interface is told the line itself, to draw as it draws a note: never the trace's rows,
    // which carry stderr's colour.
    assert!(lock(&told).is_empty());
    assert_eq!(notes, ["  · m/x · 12,345 in · 67 out · $0.0123 · 1.5s"]);
}
