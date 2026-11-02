use std::path::{Path, PathBuf};
use std::time::Duration;

use llx::daemon::build;
use llx::test_support::{Daemon, bound};
use protocol::{Done, Status};

use super::*;

const PROBE: Duration = Duration::from_millis(50);

/// A socket path in a directory of its own (made by `bind`), short enough for `sun_path`.
fn place(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("llx-s{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("d.sock")
}

#[test]
fn a_daemon_says_its_build_and_who_is_attached() {
    let path = place("status");
    assert_eq!(status(&path, PROBE), None, "nothing there");
    drop(bound(&path).unwrap());
    assert_eq!(status(&path, PROBE), None, "a stale socket");
    let daemon = Daemon::at(&path, build()).unwrap();
    let _attached = daemon.attach();
    let found = status(&path, Duration::from_secs(5)).unwrap();
    assert_eq!(
        (found.build, found.pid, found.attached),
        (build(), std::process::id(), 1)
    );
    assert!(build().starts_with(env!("CARGO_PKG_VERSION")));
}

#[test]
fn a_daemon_is_idle_only_when_nothing_needs_it() {
    let nothing = Status {
        build: build(),
        pid: 1,
        attached: 0,
        lanes: 0,
        working: 0,
        kept: 0,
        jobs: 0,
    };
    assert!(nothing.idle());
    let with = |set: fn(&mut Status)| {
        let mut status = Status {
            build: build(),
            ..nothing.clone()
        };
        set(&mut status);
        status.idle()
    };
    assert!(!with(|s| s.attached = 1), "a terminal attached");
    assert!(!with(|s| s.lanes = 1), "a session open");
    assert!(!with(|s| s.kept = 1), "sessions moved to the background");
    assert!(!with(|s| s.jobs = 1), "a monitor running");
    assert!(!with(|s| s.working = 1), "a task working");
}

/// A daemon that answers `shutdown` only after `late`, then ends: one under load.
#[expect(
    clippy::disallowed_methods,
    reason = "the test's own slow daemon: it waits on purpose before it answers"
)]
fn slow_daemon(path: &Path, late: Duration) {
    use std::io::{BufRead, BufReader, Write};
    let listener = bound(path).unwrap();
    let serving = std::thread::Builder::new().name("slow".into());
    serving
        .spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            let call: protocol::ToServer = serde_json::from_str(&line).unwrap();
            let protocol::ToServer::Call { id, .. } = call else {
                panic!("{line}")
            };
            std::thread::sleep(late);
            let done = protocol::ToClient::Done { id, done: Done::Ok };
            let mut out = &stream;
            writeln!(out, "{}", serde_json::to_string(&done).unwrap()).unwrap();
            // The daemon ends: its connection goes with it.
        })
        .unwrap();
}

#[test]
fn a_daemon_slow_to_answer_its_stop_under_load_is_still_stopped() {
    let path = place("slow-stop");
    // Past the 2 s a status is waited for (`daemon.rs`'s ASK), well within the 10 s a stop is
    // (ENDING): a stop that waited only as long as a status would miss it.
    slow_daemon(&path, Duration::from_millis(2_300));
    assert!(
        stop(&path),
        "a stop answered after a status's wait is a stop"
    );
}

/// A daemon that ends as soon as it reads `shutdown`, its connection gone before any answer.
fn quick_daemon(path: &Path) {
    use std::io::{BufRead, BufReader};
    let listener = bound(path).unwrap();
    let serving = std::thread::Builder::new().name("quick".into());
    serving
        .spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
        })
        .unwrap();
}

#[test]
fn a_daemon_that_ends_before_it_answers_its_stop_is_stopped() {
    let path = place("quick-stop");
    quick_daemon(&path);
    assert!(stop(&path), "a daemon gone is a daemon stopped");
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "a test's `ps`, which ends at once, asked whether the process is still listed"
)]
fn a_daemon_given_up_on_is_killed_and_leaves_no_zombie_behind() {
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let pid = child.id().to_string();
    given_up(&mut child);
    // A zombie is still a process `ps` lists, until its parent waits for it.
    let listed = std::process::Command::new("ps")
        .args(["-p", &pid])
        .output()
        .unwrap();
    assert!(
        !listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stdout)
    );
}
