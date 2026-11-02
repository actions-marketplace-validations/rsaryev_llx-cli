use std::sync::Mutex;
use std::time::{Duration, Instant};

use contracts::tool::Tool;
use serde_json::{Value, json};

use super::*;
use crate::shell::BashTool;
use crate::testing::{Recorder, Scratch};
use crate::workspace::Workspace;

/// A command taken: its name, its command, it running, its limit.
type Taken = (String, String, Running, Option<Duration>);

/// The session's jobs, as a test keeps them: each command taken, or none taken when `full`.
#[derive(Default)]
struct Jobs {
    taken: Mutex<Vec<Taken>>,
    full: bool,
}

impl Background for Jobs {
    fn adopt(
        &self,
        name: &str,
        command: &str,
        running: Running,
        limit: Option<Duration>,
    ) -> Option<Adopted> {
        if self.full {
            return None;
        }
        let mut taken = crate::lock(&self.taken);
        taken.push((name.into(), command.into(), running, limit));
        Some(Adopted {
            id: format!("job-{}", taken.len()),
            name: name.into(),
        })
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "a test helper takes its arguments as the tests write them, by value"
)]
#[expect(
    clippy::unwrap_in_result,
    reason = "a test's helper: a setup that fails, fails the test"
)]
fn bash(dir: &Scratch, jobs: &Jobs, arguments: Value) -> Result<String, ToolError> {
    let workspace = Workspace::new(dir.path()).unwrap();
    let tool = BashTool {
        workspace: &workspace,
        gate: &Recorder::default(),
        hidden: &Vec::new,
        stop: &|| false,
        background: Some(jobs),
    };
    tool.call(&arguments).map(|output| output.report)
}

use contracts::tool::ToolError;

/// The report of the job's command once it ended, waited for at most 10 s.
fn ended(jobs: &Jobs) -> String {
    let mut taken = crate::lock(&jobs.taken);
    let (_, _, running, _) = taken.first_mut().expect("a job was taken");
    running.report(Instant::now() + Duration::from_secs(10), &|| false)
}

#[test]
fn a_command_still_running_after_its_wait_moves_to_the_background_with_what_it_printed() {
    let (dir, jobs) = (Scratch::new(), Jobs::default());
    let started = Instant::now();
    let report = bash(
        &dir,
        &jobs,
        json!({"command": "echo first; sleep 1; echo second", "timeout_ms": 200,
               "description": "slow echo"}),
    )
    .unwrap();
    // Waited for its wait only, not for its end.
    assert!(started.elapsed() < Duration::from_secs(1), "{report}");
    assert!(
        report.starts_with(
            "Still running after: 0.2 seconds (moved to the background as job \
             `slow echo` (job-1)"
        ),
        "{report}"
    );
    assert!(report.ends_with("Output so far:\nfirst"), "{report}");
    let file = report
        .lines()
        .find_map(|line| line.strip_prefix("Whole output: "))
        .and_then(|rest| rest.split(" (").next())
        .expect("the file is named")
        .to_string();
    {
        let taken = crate::lock(&jobs.taken);
        let (name, command, _, limit) = &taken[0];
        assert_eq!(
            (name.as_str(), command.as_str(), *limit),
            ("slow echo", "echo first; sleep 1; echo second", None)
        );
    }
    // It ran on to its end in the background, and its report says so, as `bash` says it.
    let end = ended(&jobs);
    assert!(end.starts_with("Exit code: 0\n"), "{end}");
    // The file stays named: what was cut of a long output is read there.
    assert!(
        end.contains(&format!(
            "Output:\nfirst\nsecond\n... the whole output, long lines too, is in {file} ..."
        )),
        "{end}"
    );
    // The file named holds the whole output.
    assert_eq!(std::fs::read_to_string(file).unwrap(), "first\nsecond\n");
}

#[test]
fn a_command_that_ends_within_its_wait_is_reported_as_ever() {
    let (dir, jobs) = (Scratch::new(), Jobs::default());
    let report = bash(&dir, &jobs, json!({"command": "echo done"})).unwrap();
    assert!(report.starts_with("Exit code: 0\n"), "{report}");
    assert!(crate::lock(&jobs.taken).is_empty());
}

#[test]
fn run_in_background_starts_a_job_at_once_with_the_limit_asked() {
    let (dir, jobs) = (Scratch::new(), Jobs::default());
    let started = Instant::now();
    let report = bash(
        &dir,
        &jobs,
        json!({"command": "sleep 1; echo up", "run_in_background": true,
               "timeout_ms": 60_000}),
    )
    .unwrap();
    assert!(started.elapsed() < Duration::from_millis(500), "{report}");
    assert!(
        report.starts_with("Running in the background: job `sleep 1;` (job-1); its end comes back"),
        "{report}"
    );
    assert!(report.contains("\nWhole output: "), "{report}");
    let limit = crate::lock(&jobs.taken)[0].3;
    assert_eq!(limit, Some(Duration::from_secs(60)));
    let end = ended(&jobs);
    assert!(end.contains("Output:\nup\n"), "{end}");
}

#[test]
fn a_sleep_that_outlasts_its_wait_is_killed_not_kept() {
    let (dir, jobs) = (Scratch::new(), Jobs::default());
    let report = bash(
        &dir,
        &jobs,
        json!({"command": "sleep 30", "timeout_ms": 100}),
    )
    .unwrap();
    assert!(report.starts_with("Timed out after: "), "{report}");
    assert!(crate::lock(&jobs.taken).is_empty());
}

#[test]
fn a_session_that_takes_no_more_jobs_kills_the_command() {
    let dir = Scratch::new();
    let jobs = Jobs {
        full: true,
        ..Jobs::default()
    };
    let marker = dir.path().join("still-ran");
    let command = format!("echo go; sleep 1; touch {}", marker.display());
    let failed = bash(&dir, &jobs, json!({"command": command, "timeout_ms": 100}));
    assert!(
        matches!(&failed, Err(ToolError::Failed(why)) if why.starts_with("the session takes no more jobs")),
        "{failed:?}"
    );
    // Killed with everything it started: what it would have done later never happens.
    #[expect(
        clippy::disallowed_methods,
        reason = "a test waits out the command it killed"
    )]
    std::thread::sleep(Duration::from_millis(1500));
    assert!(!marker.exists());
}

#[test]
fn an_agent_with_no_jobs_cannot_start_one() {
    let dir = Scratch::new();
    let workspace = Workspace::new(dir.path()).unwrap();
    let tool = BashTool {
        workspace: &workspace,
        gate: &Recorder::default(),
        hidden: &Vec::new,
        stop: &|| false,
        background: None,
    };
    let failed = tool.call(&json!({"command": "echo x", "run_in_background": true}));
    assert!(
        matches!(&failed, Err(ToolError::Failed(why)) if why.starts_with("this session keeps no jobs")),
        "{failed:?}"
    );
}

#[test]
fn a_job_is_named_by_its_description_else_its_first_words() {
    assert_eq!(name(Some(" tests "), "bun test"), "tests");
    assert_eq!(name(None, "bun test --watch"), "bun test");
    assert_eq!(name(Some(""), "make"), "make");
    assert!(sleeps("sleep 5 && echo x"));
    assert!(!sleeps("bun test"));
}
