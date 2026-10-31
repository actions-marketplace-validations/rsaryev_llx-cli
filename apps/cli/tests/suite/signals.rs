//! What a signal does to a run at no terminal (`exec`): SIGHUP and SIGTERM stop the task as
//! Ctrl-C does, and a second signal quits the process without leaving what its tools started
//! behind.

use std::path::Path;
use std::process::{Child, Stdio};
use std::time::Duration;

use serde_json::json;

use crate::common::eventually;
use crate::common::home::{Home, detach};
use crate::common::model::{Model, Reply};

/// How often a test looks again at what it waits for.
const EVERY: Duration = Duration::from_millis(20);

#[expect(
    clippy::disallowed_methods,
    reason = "a pause between two signals, so the first is taken before the second comes"
)]
fn pause(millis: u64) {
    std::thread::sleep(Duration::from_millis(millis));
}

fn start(home: &Home, model: &Model) -> Child {
    let mut command = home.with_model(model, &["exec", "-y", "--timeout", "120", "x"]);
    detach(&mut command);
    command
        .current_dir(home.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

/// The pid the command wrote to `file`, once it has.
fn pid_in(file: &Path) -> i32 {
    let written = || std::fs::read_to_string(file).ok()?.trim().parse().ok();
    let pid = eventually(Duration::from_secs(20), EVERY, written);
    pid.unwrap_or_else(|| panic!("the command never wrote its pid"))
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

fn gone_soon(pid: i32) -> bool {
    eventually(Duration::from_secs(10), EVERY, || {
        (!alive(pid)).then_some(())
    })
    .is_some()
}

/// How the process ended, as a shell reads it (128 and the signal for a death by one).
fn exit_of(child: &mut Child) -> Option<i32> {
    let status = eventually(Duration::from_secs(15), EVERY, || child.try_wait().unwrap())?;
    let signal = std::os::unix::process::ExitStatusExt::signal(&status);
    Some(signal.map_or_else(|| status.code().unwrap(), |signal| 128 + signal))
}

fn signal(child: &Child, signal: libc::c_int) {
    // SAFETY: kill only sends a signal to a process the test started.
    unsafe { libc::kill(child.id().cast_signed(), signal) };
}

#[test]
fn a_hangup_stops_the_task_and_the_command_it_runs() {
    let home = Home::new();
    let file = home.path().join("pid");
    let command = format!("echo $$ > {}; exec sleep 300", file.display());
    let model = Model::start(vec![Reply::call("bash", json!({ "command": command }))]);
    let mut child = start(&home, &model);
    let pid = pid_in(&file);
    signal(&child, libc::SIGHUP);
    assert_eq!(
        exit_of(&mut child),
        Some(130),
        "a hangup stops, it does not kill"
    );
    assert!(gone_soon(pid), "the command outlived the run");
}

#[test]
fn a_second_ctrl_c_quits_and_takes_the_monitors_with_it() {
    let home = Home::new();
    let file = home.path().join("pid");
    let command = format!("echo $$ > {}; exec sleep 300", file.display());
    let model = Model::start(vec![
        Reply::call(
            "jobs",
            json!({"action": "monitor", "name": "m", "command": command}),
        ),
        // The model never answers: the task cannot end by itself.
        Reply::After(home.path().join("never"), Box::new(Reply::done("late"))),
    ]);
    let mut child = start(&home, &model);
    let pid = pid_in(&file);
    signal(&child, libc::SIGINT);
    pause(300);
    signal(&child, libc::SIGINT);
    assert_eq!(exit_of(&mut child), Some(130));
    assert!(gone_soon(pid), "the monitor outlived the run");
}
