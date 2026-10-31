//! Real processes: jobs a shell with job control put in groups of their own, which a signal to
//! the shell's group never reaches.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Stdio};

use super::*;
use crate::shell::process;

/// Whether `pid` is gone within two seconds (a killed orphan is reaped by init).
#[expect(
    clippy::disallowed_methods,
    reason = "a test looks again until it is so, for a bounded time: there is no event to wait on"
)]
fn gone(pid: libc::pid_t) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    // SAFETY: signal 0 only asks whether the process exists.
    while unsafe { libc::kill(pid, 0) } == 0 {
        if Instant::now() > deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(20));
    }
    true
}

fn group(pid: libc::pid_t) -> libc::pid_t {
    // SAFETY: getpgid only reads which group a process is in.
    unsafe { libc::getpgid(pid) }
}

/// `command` in a shell that leads a session of its own, as `bash` runs it; the pid it prints
/// first.
fn spawn(command: &str) -> (Child, libc::pid_t) {
    let mut child = process::shell(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.as_mut().unwrap())
        .read_line(&mut line)
        .unwrap();
    (child, line.trim().parse().unwrap())
}

#[test]
fn no_session_is_swept_for_init_or_nothing() {
    assert_eq!(kill_members(0), Pass::default());
    assert_eq!(kill_members(1), Pass::default());
}

#[test]
fn the_sweep_kills_the_members_and_spares_the_leader() {
    // The shell reaps the sleep it started once the sweep kills it (a killed child nobody waits
    // for stays a zombie of the session), then waits on its stdin: it outlives the sweep.
    let (mut shell, sleep) = spawn("sleep 300 & echo $!; wait; read line");
    let leader = shell.id().cast_signed();
    assert!(end(leader, Duration::from_secs(2)));
    assert!(gone(sleep), "the sleep still runs");
    assert_eq!(shell.try_wait().unwrap(), None, "the leader was killed");
    shell.stdin.as_mut().unwrap().write_all(b"\n").unwrap();
    shell.wait().unwrap();
}

#[test]
fn a_job_in_a_group_of_its_own_dies_with_its_command() {
    // `set -m`: job control, so the sleep gets a group of its own, as in an interactive shell.
    let (mut shell, sleep) = spawn("set -m; sleep 300 & echo $!; wait");
    let leader = shell.id().cast_signed();
    if group(sleep) == leader {
        eprintln!("this sh has no job control without a terminal: the group kill alone covers it");
    }
    assert!(process::kill(&mut shell), "the kill was not complete");
    assert!(gone(sleep), "the job outlived its command");
}

/// How many processes the host runs is not the test's to fix, so the table's size is not checked
/// against it; the listing is: this process and a child it just started are both in it.
#[test]
fn the_listing_holds_this_process_and_its_child() {
    let (mut shell, _) = spawn("echo $$; read line");
    let child = shell.id().cast_signed();
    let mut listed = vec![];
    each_pid(|pid| listed.push(pid));
    shell.stdin.as_mut().unwrap().write_all(b"\n").unwrap();
    shell.wait().unwrap();
    assert!(
        listed.contains(&(std::process::id().cast_signed())),
        "this process"
    );
    assert!(listed.contains(&child), "its child");
}
