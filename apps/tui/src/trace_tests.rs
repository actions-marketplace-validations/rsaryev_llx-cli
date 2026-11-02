use protocol::{Event, Outcome};

use super::*;
use crate::test_support::ran;

/// A call of `bash` running `command`, and its result: it printed nothing.
fn call(command: &str) -> [Event; 2] {
    let label = ran(command);
    let call = Event::Call {
        call: "c1".into(),
        name: "bash".into(),
        label: label.clone(),
    };
    let result = Event::Result {
        call: "c1".into(),
        name: "bash".into(),
        label,
        outcome: Outcome::Done {
            report: "Exit code: 0\nWall time: 1.0 seconds\nOutput:".into(),
            shown: String::new(),
        },
    };
    [call, result]
}

fn drawn(trace: &mut Trace, events: &[Event]) -> String {
    events.iter().map(|event| trace.draw(event)).collect()
}

#[test]
fn a_person_reads_each_call_once_done_then_how_the_task_ended() {
    let [call, result] = call("cargo test");
    let end = Event::TaskEnded {
        status: "done".into(),
        millis: 10,
        turns: 0,
        cost: 0.0,
    };
    let spend = Event::Note("session  1-ab\n".into());
    let trace = drawn(&mut Trace::new(false, 100), &[call, result, end, spend]);
    insta::assert_snapshot!(trace, @r"
    • Ran cargo test
      └ (no output)

    ─ Worked for 0.0s · 0 turns · $0.0000 ──────────────────────────────────────────────────────────────
      session  1-ab
    ");
}

#[test]
fn the_steps_under_a_call_are_drawn_with_it_and_only_with_it() {
    let [call, result] = call("ls");
    let events = [
        Event::Step("left from before".into()),
        call,
        Event::Step("a: Ran cargo build".into()),
        result,
        Event::Answer("Built.".into()),
        Event::Error("the model is gone".into()),
    ];
    let trace = drawn(&mut Trace::new(false, 100), &events);
    insta::assert_snapshot!(trace, @r"
    • Ran ls
      · a: Ran cargo build
      └ (no output)

    • Built.
    ■ the model is gone
    ");
}

#[test]
fn a_turn_for_v_is_a_quiet_line_and_what_the_trace_does_not_show_adds_nothing() {
    let mut trace = Trace::new(false, 100);
    let turn = Event::Note("  · m/x · 12,345 in · 67 out · $0.0123 · 1.5s".into());
    assert_eq!(
        trace.draw(&turn),
        "    · m/x · 12,345 in · 67 out · $0.0123 · 1.5s\n"
    );
    for event in [
        Event::Spent(10),
        Event::TaskStarted("x".into()),
        Event::Quit,
    ] {
        assert_eq!(trace.draw(&event), "");
    }
}

#[test]
fn colour_is_drawn_as_the_terminal_reads_it() {
    let [call, result] = call("echo hi");
    let trace = drawn(&mut Trace::new(true, 100), &[call, result]);
    assert_eq!(
        trace,
        "\u{1b}[32m• \u{1b}[0m\u{1b}[1mRan\u{1b}[0m\u{1b}[1m \u{1b}[0mecho hi\n\
         \u{1b}[2m  └ \u{1b}[0m\u{1b}[2m(no output)\u{1b}[0m\n"
    );
}

#[test]
fn the_answer_at_a_terminal_comes_after_a_blank_row_as_the_model_wrote_it_drawn() {
    // The tests' stdout is no terminal: no colour.
    let answer = Trace::new(false, 40).answer();
    assert_eq!(answer("All **good**."), "\n• All good.\n");
}
