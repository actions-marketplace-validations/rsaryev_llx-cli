use std::process::Stdio;

use super::*;

fn start() -> Child {
    super::super::shell("sleep 60")
        .stdin(Stdio::null())
        .spawn()
        .unwrap()
}

fn listed(pid: libc::pid_t) -> bool {
    LIVE.lock().unwrap().contains(&pid)
}

#[test]
fn a_session_is_listed_from_its_start_until_it_is_dropped() {
    let mut child = start();
    let pid = child.id().cast_signed();
    let tracked = track(&child);
    assert!(listed(pid));
    drop(tracked);
    assert!(!listed(pid));
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn killing_a_session_ends_the_command_and_leaves_another_running() {
    let (mut doomed, mut spared) = (start(), start());
    kill(&[doomed.id().cast_signed()]);
    assert!(
        doomed.wait().unwrap().code().is_none(),
        "killed by a signal"
    );
    assert!(spared.try_wait().unwrap().is_none(), "still running");
    spared.kill().unwrap();
    spared.wait().unwrap();
}
