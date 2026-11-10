use contracts::history::{Session, Spent, Step, Turn};

use super::*;

fn model(agent: &str, (input, cached): (u64, u64), cost: f64, at_ms: u64) -> Step {
    Step {
        what: "model".into(),
        agent: agent.into(),
        model: "m".into(),
        at_ms,
        ms: 1_000,
        spent: Spent {
            input_tokens: input,
            cached_tokens: cached,
            output_tokens: 100,
            cost,
            ..Spent::default()
        },
        ..Step::default()
    }
}

fn elided(agent: &str) -> Step {
    Step {
        what: "elided".into(),
        agent: agent.into(),
        detail: "1 outputs".into(),
        ..Step::default()
    }
}

fn session(steps: Vec<Step>) -> Session {
    Session {
        id: "s1".into(),
        turns: vec![Turn {
            goal: "go on".into(),
            answer: String::new(),
            status: "done".into(),
            at: 0,
            steps,
        }],
        ..Session::default()
    }
}

#[test]
fn a_stream_misses_after_an_elision_and_its_first_request_is_cold() {
    let mut streams = Streams::default();
    let spent = |input, cached| Spent {
        input_tokens: input,
        cached_tokens: cached,
        ..Spent::default()
    };
    let cold = streams.request("", "m", (0, 10), &spent(100_000, 0));
    assert_eq!(cold.map(|miss| miss.cause), Some(Cause::Cold));
    assert_eq!(
        streams.request("", "m", (20, 10), &spent(110_000, 100_000)),
        None
    );
    streams.elided("");
    let miss = streams
        .request("", "m", (40, 10), &spent(80_000, 5_000))
        .unwrap();
    assert_eq!((miss.cause, miss.tokens), (Cause::Elided, 75_000));
}

#[test]
fn each_agent_is_its_own_stream_and_a_long_pause_or_a_new_model_is_named() {
    let mut streams = Streams::default();
    let spent = |input, cached| Spent {
        input_tokens: input,
        cached_tokens: cached,
        ..Spent::default()
    };
    streams.request("a", "m", (0, 10), &spent(50_000, 0));
    streams.request("b", "m", (0, 10), &spent(90_000, 0));
    // `a` grows from its own last request, not from `b`'s.
    let paused = streams.request("a", "m", (miss::LIFETIME_MS + 10, 10), &spent(52_000, 0));
    assert_eq!(paused.map(|miss| miss.cause), Some(Cause::Pause));
    let switched = streams.request("b", "n", (30, 10), &spent(91_000, 0));
    assert_eq!(switched.map(|miss| miss.cause), Some(Cause::Model));
}

#[test]
fn the_report_sums_by_task_agent_and_model_and_lists_the_misses() {
    let shown = report(&session(vec![
        model("", (60_000, 0), 0.15, 0),
        model("", (61_000, 60_000), 0.01, 2_000),
        model("policy", (60_000, 59_000), 0.01, 3_000),
        model("policy", (200_000, 198_000), 0.03, 4_000),
        elided("policy"),
        model("policy", (150_000, 9_000), 0.45, 6_000),
    ]));
    assert!(
        shown.starts_with("s1\ntotal      5 requests · $0.650"),
        "{shown}"
    );
    assert!(shown.contains("  main "), "{shown}");
    assert!(shown.contains("  policy "), "{shown}");
    assert!(shown.contains("cache misses: 2"), "{shown}");
    assert!(
        shown.contains("1 ×   $0.450  after the context was elided"),
        "{shown}"
    );
    assert!(shown.contains("task 1   policy"), "{shown}");
}

#[test]
fn a_count_just_short_of_a_million_reads_as_one_million() {
    let shown = report(&session(vec![model("", (999_960, 0), 2.5, 0)]));
    assert!(shown.contains("1.0M"), "{shown}");
    assert!(!shown.contains("1000k"), "{shown}");
}

#[test]
fn a_session_that_spent_nothing_says_so() {
    assert_eq!(report(&session(vec![])), "s1\nNothing spent yet.\n");
}

#[test]
fn a_cost_under_a_cent_keeps_the_digits_that_tell_it_from_nothing() {
    // The bug: every request of a cheap session read `$0.000`.
    let shown = report(&session(vec![model("", (8_000, 7_800), 0.0003, 100)]));
    assert!(shown.contains("$0.0003"), "{shown}");
    assert!(!shown.contains("$0.000 "), "{shown}");
}
