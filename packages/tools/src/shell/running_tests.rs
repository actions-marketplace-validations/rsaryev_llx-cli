use std::time::{Duration, Instant};

use super::*;
use crate::shell::process::{self, Job, Quiet};
use crate::shell::sandbox::Scope;
use crate::testing::Scratch;
use crate::workspace::Workspace;

fn started(dir: &Scratch, command: &str) -> Running {
    let workspace = Workspace::new(dir.path()).unwrap();
    let job = Job {
        command,
        dir: dir.path(),
        timeout: Duration::from_secs(10),
        hidden: &[],
        stop: &|| false,
        scope: Scope::Open,
        workspace: &workspace,
    };
    let _ = Quiet::On;
    process::start(&job).unwrap()
}

#[test]
fn a_command_let_go_before_its_end_is_killed_with_all_it_started() {
    let dir = Scratch::new();
    let marker = dir.path().join("still-ran");
    let running = started(
        &dir,
        &format!("(sleep 1; touch {}) & wait", marker.display()),
    );
    drop(running);
    #[expect(
        clippy::disallowed_methods,
        reason = "a test waits out the command it killed"
    )]
    std::thread::sleep(Duration::from_millis(1500));
    assert!(!marker.exists());
}

#[test]
fn a_wait_that_ends_first_leaves_the_command_running_to_be_waited_for_again() {
    let dir = Scratch::new();
    let mut running = started(&dir, "echo a; sleep 0.5; echo b");
    let early = running.until(Instant::now() + Duration::from_millis(100), &|| false);
    assert!(matches!(early, Ok(None)), "{early:?}");
    assert_eq!(running.so_far(), "a");
    let report = running.report(Instant::now() + Duration::from_secs(10), &|| false);
    assert!(report.starts_with("Exit code: 0\n"), "{report}");
    assert!(report.ends_with("Output:\na\nb"), "{report}");
}
