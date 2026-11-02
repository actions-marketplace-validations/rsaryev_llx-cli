use std::thread;
use std::time::{Duration, Instant};

use super::*;
use crate::shell::shell;
use crate::testing::Scratch;

const WAIT: Duration = Duration::from_secs(10);

#[test]
fn a_query_answers_with_what_it_printed_on_stdout_alone() {
    let answer = query(shell("echo hidden >&2; printf 'one\\ntwo\\n'"), WAIT).unwrap();
    assert_eq!(answer, b"one\ntwo\n");
}

#[test]
fn a_query_that_ends_badly_says_how() {
    let failed = query(shell("echo partial; exit 3"), WAIT).unwrap_err();
    assert_eq!(failed.to_string(), "exited 3");
    let killed = query(shell("kill -9 $$"), WAIT).unwrap_err();
    assert_eq!(killed.to_string(), "was killed by signal 9");
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test looks again until it is so, for a bounded time: there is no event to wait on"
)]
#[test]
fn a_late_query_is_killed_with_what_it_started() {
    let dir = Scratch::new();
    let mut command = shell("sleep 30 & echo $! > child.pid; wait");
    command.current_dir(dir.path());
    let started = Instant::now();
    let late = query(command, Duration::from_millis(300)).unwrap_err();
    assert!(matches!(late, Unanswered::Late(_)), "{late}");
    assert!(started.elapsed() < Duration::from_secs(5), "{late}");
    let pid: libc::pid_t = std::fs::read_to_string(dir.path().join("child.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // The orphan is reaped by init shortly after it dies.
    let deadline = Instant::now() + Duration::from_secs(2);
    // SAFETY: signal 0 only asks whether the process exists.
    while unsafe { libc::kill(pid, 0) } == 0 {
        assert!(
            Instant::now() < deadline,
            "the background sleep outlived the deadline"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

/// Past what is kept, the rest is read and dropped: the command ends as it would, not on a
/// pipe closed under it.
#[test]
fn a_query_that_prints_more_than_is_kept_still_ends_well() {
    // A MiB past it, more than a pipe holds: a reader gone at the cap would cut the command off.
    let printed = format!("head -c {} /dev/zero", MOST + (1 << 20));
    let answer = query(shell(&printed), WAIT).unwrap();
    assert_eq!(answer.len(), MOST);
}

#[test]
fn a_query_that_cannot_start_says_so() {
    let mut command = shell("true");
    command.current_dir("/nowhere/at/all");
    let failed = query(command, WAIT).unwrap_err();
    assert!(
        failed.to_string().starts_with("did not start: "),
        "{failed}"
    );
}
