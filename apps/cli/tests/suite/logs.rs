//! The log: what a run did, in `~/.config/llx/logs/llx.log`, as Rust's `log` facade says it, and
//! `RUST_LOG` choosing what is kept.

use serde_json::json;

use crate::common::home::Home;
use crate::common::model::{Model, Reply};

/// A task whose one call fails: `bash` in a `workdir` outside the workspace.
fn a_failing_call() -> Model {
    Model::start(vec![
        Reply::call(
            "bash",
            json!({"command": "true", "workdir": "../elsewhere"}),
        ),
        Reply::done("Nothing ran."),
    ])
}

fn log(home: &Home) -> Option<String> {
    std::fs::read_to_string(home.path().join("config/llx/logs/llx.log")).ok()
}

#[test]
fn a_task_leaves_each_call_with_its_arguments_its_failure_and_how_it_ended_in_the_log() {
    let (home, model) = (Home::new(), a_failing_call());
    let out = home.exec(&model, &["exec", "-y", "run nothing"], "");
    assert_eq!(out.status.code(), Some(0));
    let log = log(&home).expect("the log is written");
    // The call as the model made it: what a bug report needs, and what the screen leaves out.
    assert!(
        log.contains(r#"bash {"command":"true","workdir":"../elsewhere"}"#),
        "{log}"
    );
    let failed = log
        .lines()
        .find(|line| line.contains("failed") && line.contains("elsewhere"));
    assert!(failed.is_some_and(|line| line.contains(" WARN ")), "{log}");
    assert!(log.contains("done"), "{log}");
    // One event a line, each with its time and level first, as env_logger writes them.
    for line in log.lines() {
        assert!(line.starts_with("[20") && line.contains("Z "), "{line}");
    }
    {
        use std::os::unix::fs::PermissionsExt;
        let file = home.path().join("config/llx/logs/llx.log");
        let mode = std::fs::metadata(file).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "the person's alone: it holds what the tools read"
        );
    }
}

#[test]
fn rust_log_chooses_what_is_kept() {
    let (home, model) = (Home::new(), a_failing_call());
    let mut command = home.with_model(&model, &["exec", "-y", "run nothing"]);
    command.env("RUST_LOG", "off");
    let _ = home.exec_command(command, "");
    assert!(
        log(&home).unwrap_or_default().is_empty(),
        "off keeps nothing"
    );

    let (home, model) = (Home::new(), a_failing_call());
    let mut command = home.with_model(&model, &["exec", "-y", "run nothing"]);
    command.env("RUST_LOG", "llx=debug");
    let _ = home.exec_command(command, "");
    let log = log(&home).expect("the log is written");
    // Debug adds the model's turns and what it said.
    assert!(
        log.contains(" DEBUG ") && log.contains("Nothing ran."),
        "{log}"
    );
}
