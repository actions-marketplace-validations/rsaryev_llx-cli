use agent::agent::Status;
use serde_json::json;

use super::*;
use crate::exit;

const LIMITS: Limits = Limits {
    time: std::time::Duration::from_secs(90),
    budget: Some(0.5),
};

#[test]
fn an_answer_ends_well_and_running_out_or_failing_says_so() {
    let ended = |status: Status| Ended::of(&status, None, None, LIMITS);
    assert_eq!(
        (ended(Status::Done).code, ended(Status::Done).status),
        (exit::OK, "done")
    );
    let exhausted = ended(Status::Exhausted);
    assert_eq!(exhausted.code, exit::EXHAUSTED);
    assert!(exhausted.error.unwrap().contains("--max-turns"));
    let error = ended(Status::Error("no network".into()));
    assert_eq!(
        error,
        Ended {
            status: "error",
            code: exit::ENVIRONMENT,
            error: Some("no network".into())
        }
    );
}

#[test]
fn data_that_does_not_match_fails_the_run() {
    let mismatch = Err("the result did not match the schema".to_string());
    let ended = Ended::of(&Status::Done, Some(&mismatch), None, LIMITS);
    assert_eq!((ended.code, ended.status), (exit::FAILED, "no_data"));
    let data = Ok(json!({"n": 1}));
    assert_eq!(
        Ended::of(&Status::Done, Some(&data), None, LIMITS).code,
        exit::OK
    );
}

#[test]
fn a_halt_decides_the_status_and_exit_code() {
    // Whatever the model said, the halt decides.
    let model_unreachable = Status::Error("the time limit ran out".into());
    let stopped = Status::Error("the person stopped the task".into());
    for (status, halt, ended, code, error) in [
        (
            &model_unreachable,
            Halt::TimedOut,
            "exhausted",
            exit::EXHAUSTED,
            Some("90 s"),
        ),
        (
            &model_unreachable,
            Halt::Closed,
            "interrupted",
            exit::OK,
            None,
        ),
        (
            &stopped,
            Halt::Interrupted,
            "interrupted",
            exit::INTERRUPTED,
            None,
        ),
    ] {
        let got = Ended::of(status, None, Some(halt), LIMITS);
        assert_eq!((got.status, got.code), (ended, code), "{halt:?}");
        match error {
            Some(part) => assert!(got.error.unwrap().contains(part), "{halt:?}"),
            None => assert_eq!(got.error, None, "{halt:?}"),
        }
    }
}

#[test]
fn a_spent_budget_exhausts_the_task_and_says_how_to_raise_it() {
    let ended = Ended::of(&Status::Done, None, Some(Halt::OverBudget), LIMITS);
    assert_eq!(ended.code, exit::EXHAUSTED);
    assert_eq!(ended.status, "exhausted");
    assert_eq!(
        ended.error.as_deref(),
        Some("the budget ran out ($0.5; raise it with --budget)")
    );
}
