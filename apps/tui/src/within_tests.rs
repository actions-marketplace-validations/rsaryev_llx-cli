use std::process::Command;
use std::time::Duration;

use super::{exits_within, within};

#[test]
fn a_child_is_waited_for_until_its_deadline_then_killed() {
    let mut echo = Command::new("sh");
    echo.args(["-c", "echo one; echo two"]);
    let out = within(echo, Duration::from_secs(10));
    assert_eq!(out.unwrap(), b"one\ntwo\n");
    let mut failed = Command::new("sh");
    failed.args(["-c", "echo half; exit 3"]);
    assert_eq!(within(failed, Duration::from_secs(10)), None);
    let mut stuck = Command::new("sh");
    stuck.args(["-c", "exec sleep 30"]);
    let asked = std::time::Instant::now();
    assert_eq!(within(stuck, Duration::from_millis(200)), None);
    assert!(
        asked.elapsed() < Duration::from_secs(5),
        "{:?}",
        asked.elapsed()
    );
}

#[test]
fn a_child_waited_for_to_exit_fails_on_a_bad_status_and_is_killed_at_its_deadline() {
    let mut failed = Command::new("sh");
    failed.args(["-c", "exit 3"]);
    assert!(!exits_within(failed, vec![], Duration::from_secs(10)));
    let mut stuck = Command::new("sh");
    stuck.args(["-c", "exec sleep 30"]);
    let asked = std::time::Instant::now();
    assert!(!exits_within(stuck, vec![], Duration::from_millis(200)));
    assert!(
        asked.elapsed() < Duration::from_secs(5),
        "{:?}",
        asked.elapsed()
    );
}

#[test]
fn what_the_child_is_given_never_holds_the_caller_when_it_never_reads_it() {
    let mut deaf = Command::new("sh");
    deaf.args(["-c", "exec sleep 30"]);
    let big = vec![b'x'; 4 << 20];
    assert!(!exits_within(deaf, big, Duration::from_millis(200)));
}

#[test]
fn what_the_child_is_given_reaches_it() {
    let given = |input: &[u8]| {
        let mut check = Command::new("sh");
        check.args(["-c", r#"test "$(cat)" = copied"#]);
        exits_within(check, input.to_vec(), Duration::from_secs(10))
    };
    assert!(given(b"copied"));
    assert!(!given(b"something else"));
}
