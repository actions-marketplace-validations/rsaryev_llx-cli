//! How a task ended, as the report and the exit code say it.

use agent::agent::Status;
use serde_json::Value;

use crate::brake::{Halt, Limits};
use crate::exit;

/// How a task ended, for the report and the exit code.
#[derive(Debug, PartialEq)]
pub(crate) struct Ended {
    pub status: &'static str,
    pub code: i32,
    pub error: Option<String>,
}

impl Ended {
    /// How a task that ran within `limits` ended: they give the words of the one it hit.
    pub(crate) fn of(
        status: &Status,
        data: Option<&Result<Value, String>>,
        halt: Option<Halt>,
        limits: Limits,
    ) -> Self {
        let ended = |status, code, error: Option<String>| Self {
            status,
            code,
            error,
        };
        match (halt, status) {
            (Some(Halt::TimedOut), _) => ended(
                "exhausted",
                exit::EXHAUSTED,
                Some(format!(
                    "the time limit ran out ({} s; raise it with --timeout)",
                    limits.time.map_or(0, |time| time.as_secs())
                )),
            ),
            (Some(Halt::OverBudget), _) => ended(
                "exhausted",
                exit::EXHAUSTED,
                Some(format!(
                    "the budget ran out (${}; raise it with --budget)",
                    limits.budget.unwrap_or_default()
                )),
            ),
            (Some(Halt::Closed), _) => ended("interrupted", exit::OK, None),
            (Some(Halt::Interrupted), _) => ended("interrupted", exit::INTERRUPTED, None),
            (None, Status::Error(error)) => ended("error", exit::ENVIRONMENT, Some(error.clone())),
            (None, Status::Exhausted) => ended(
                "exhausted",
                exit::EXHAUSTED,
                Some("out of turns before an answer (raise them with --max-turns)".into()),
            ),
            // `--schema` asked for data and none matched: the one way an answer fails.
            (None, Status::Done) if matches!(data, Some(Err(_))) => {
                ended("no_data", exit::FAILED, None)
            }
            (None, Status::Done) => ended("done", exit::OK, None),
        }
    }
}

#[cfg(test)]
#[path = "ended_tests.rs"]
mod tests;
