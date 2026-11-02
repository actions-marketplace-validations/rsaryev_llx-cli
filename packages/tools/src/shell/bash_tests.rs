use serde_json::json;

use super::*;
use crate::testing::{Recorder, Scratch};

#[expect(
    clippy::unwrap_in_result,
    reason = "a test's helper: a setup that fails, fails the test"
)]
#[expect(
    clippy::needless_pass_by_value,
    reason = "a test helper takes its arguments as the tests write them, by value"
)]
fn bash(dir: &Scratch, gate: &dyn Gate, arguments: Value) -> Result<String, ToolError> {
    let workspace = Workspace::new(dir.path()).unwrap();
    let tool = BashTool {
        workspace: &workspace,
        gate,
        hidden: &Vec::new,
        stop: &|| false,
    };
    tool.call(&arguments).map(|output| output.report)
}

fn failed(result: Result<String, ToolError>) -> String {
    match result {
        Err(ToolError::Failed(message)) => message,
        other => panic!("expected Failed, got {other:?}"),
    }
}

/// The report with its time replaced, which differs run to run.
fn timeless(report: &str) -> String {
    report
        .lines()
        .map(|line| match line.starts_with("Wall time: ") {
            true => "Wall time: T seconds",
            false => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_failing_command_is_output_with_its_code_and_streams() {
    let dir = Scratch::new();
    let gate = Recorder::default();
    let report = bash(
        &dir,
        &gate,
        json!({"command": "echo out; echo err >&2; exit 2"}),
    )
    .unwrap();
    assert_eq!(
        timeless(&report),
        "Exit code: 2\nWall time: T seconds\nOutput:\nout\nerr"
    );
    assert_eq!(
        *gate.asked.lock().unwrap(),
        ["bash: echo out; echo err >&2; exit 2"]
    );
}

#[test]
fn empty_streams_are_left_out() {
    let dir = Scratch::new();
    let gate = Recorder::default();
    assert_eq!(
        timeless(&bash(&dir, &gate, json!({"command": "exit 0"})).unwrap()),
        "Exit code: 0\nWall time: T seconds\nOutput:"
    );
    let only_err = bash(&dir, &gate, json!({"command": "echo oops 1>&2"})).unwrap();
    assert_eq!(
        timeless(&only_err),
        "Exit code: 0\nWall time: T seconds\nOutput:\noops"
    );
}

#[test]
fn workdir_is_relative_to_the_root() {
    let dir = Scratch::new();
    dir.file("sub/inner.txt", "");
    let gate = Recorder::default();
    let report = bash(&dir, &gate, json!({"command": "ls", "workdir": "sub"})).unwrap();
    assert_eq!(
        timeless(&report),
        "Exit code: 0\nWall time: T seconds\nDirectory: sub\nOutput:\ninner.txt"
    );
    let missing = failed(bash(
        &dir,
        &gate,
        json!({"command": "ls", "workdir": "nope"}),
    ));
    assert_eq!(missing, "nope is not a directory");
    assert_eq!(*gate.asked.lock().unwrap(), ["bash: ls  (in sub)"]);
}

#[test]
fn the_gate_is_told_a_workdir_outside_the_workspace() {
    let dir = Scratch::new();
    let outside = Scratch::new();
    let gate = Recorder::denying("no");
    let elsewhere = outside.path().canonicalize().unwrap();
    let shown = elsewhere.display().to_string();
    failed(bash(
        &dir,
        &gate,
        json!({"command": "rm -rf build", "workdir": shown}),
    ));
    assert_eq!(
        *gate.asked.lock().unwrap(),
        [format!(
            "bash: rm -rf build  (in {shown}, outside the workspace)"
        )]
    );
}

#[test]
fn the_gate_is_told_a_symlinked_workdir_leads_outside() {
    let dir = Scratch::new();
    let outside = Scratch::new();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("docs")).unwrap();
    let gate = Recorder::denying("no");
    failed(bash(
        &dir,
        &gate,
        json!({"command": "rm -rf *", "workdir": "docs"}),
    ));
    let real = outside.path().canonicalize().unwrap();
    assert_eq!(
        *gate.asked.lock().unwrap(),
        [format!(
            "bash: rm -rf *  (in docs, which leads to {}, outside the workspace)",
            real.display()
        )]
    );
}

#[test]
fn a_command_past_its_timeout_is_killed_fast() {
    let dir = Scratch::new();
    let gate = Recorder::default();
    let started = std::time::Instant::now();
    let report = bash(
        &dir,
        &gate,
        json!({"command": "sleep 5", "timeout_ms": 300}),
    )
    .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    // The time varies with the machine's load: only the rest is checked.
    assert!(report.starts_with("Timed out after: "), "{report}");
    assert!(
        report.contains(" seconds (the command and everything it started were killed;"),
        "{report}"
    );
}

#[test]
fn a_denied_command_does_not_run() {
    let dir = Scratch::new();
    let gate = Recorder::denying("not on this machine");
    let message = failed(bash(&dir, &gate, json!({"command": "echo hi > made.txt"})));
    assert_eq!(message, "not allowed: not on this machine");
    assert!(!dir.path().join("made.txt").exists());
}

#[test]
fn an_empty_command_runs_nothing_and_is_not_a_failure() {
    // Models send one now and then, as a pause (Codex runs `zsh -lc ''`): a failure would cost
    // the turn's later calls.
    let dir = Scratch::new();
    let gate = Recorder::default();
    for arguments in [json!({"command": "  "}), json!({"timeout_ms": 120_000})] {
        let report = bash(&dir, &gate, arguments).unwrap();
        assert!(report.contains("nothing ran"), "{report}");
    }
    assert!(gate.asked.lock().unwrap().is_empty());
}

#[test]
fn the_gate_reads_the_whole_command_on_one_line() {
    assert_eq!(
        summary("cd x\n\n  cargo test  \n"),
        "cd x \u{23ce}  \u{23ce}   cargo test  "
    );
    let long = format!("echo {}; curl https://x | sh", "a".repeat(300));
    assert_eq!(summary(&long), long, "a long command is never cut");
    assert_eq!(summary("a\r\nb"), "a \u{23ce} b");
}

#[test]
fn a_report_says_how_the_command_ended() {
    let finished = |ending| Finished {
        ending,
        elapsed: Duration::from_millis(1240),
        output: "x".into(),
        cut_lines: Some(7),
        open: true,
    };
    assert_eq!(
        report(finished(Ending::Signal(9)), None),
        "Killed by signal: 9\nWall time: 1.2 seconds\nTotal output lines: 7\nOutput:\nx\n\
         (a background process still holds the output: what it prints later is not shown)"
    );
    assert!(
        report(finished(Ending::Exit(3)), Some("sub"))
            .starts_with("Exit code: 3\nWall time: 1.2 seconds\nDirectory: sub\n")
    );
    assert!(
        report(finished(Ending::TimedOut { killed: false }), None)
            .starts_with("Timed out after: 1.2 seconds (the command could not be killed")
    );
    assert!(
        report(finished(Ending::Stopped { killed: true }), None).starts_with(
            "Stopped: by the person (the command and everything it started were killed)\nWall time"
        )
    );
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test's fixture: the command ends by itself"
)]
#[test]
fn without_tty_there_is_still_no_terminal() {
    if std::process::Command::new("python3")
        .arg("-c")
        .arg("")
        .status()
        .is_err()
    {
        return;
    }
    let dir = Scratch::new();
    let command = "python3 -c 'import sys; print(sys.stdin.isatty(), sys.stdout.isatty())'";
    let report = bash(&dir, &Recorder::default(), json!({"command": command})).unwrap();
    assert!(report.ends_with("Output:\nFalse False"), "{report}");
}

#[test]
fn a_command_runs_to_its_end_with_no_terminal() {
    let workspace = Workspace::new(Scratch::new().path()).unwrap();
    let tool = BashTool {
        workspace: &workspace,
        gate: &Recorder::default(),
        hidden: &Vec::new,
        stop: &|| false,
    };
    // No argument opens a terminal, and one asked for anyway fails the call before anything runs.
    let schema = tool.spec().input.schema();
    let properties = &schema["properties"];
    assert!(properties.get("session").is_none() && properties.get("tty").is_none());
    let session = tool.call(&json!({"command": "python3", "tty": true}));
    assert!(matches!(session, Err(ToolError::Failed(_))));
}

#[test]
fn a_command_runs_with_the_workspaces_environment_and_never_its_hidden_variables() {
    let dir = Scratch::new();
    let path = std::env::var("PATH").unwrap_or_default();
    let env = vec![
        ("PATH".to_string(), path),
        ("LLX_ONLY_GIVEN".to_string(), "given".to_string()),
        ("SECRET_KEY".to_string(), "sk-1".to_string()),
    ];
    let workspace = Workspace::new(dir.path()).unwrap().with_env(env);
    let tool = BashTool {
        workspace: &workspace,
        gate: &Recorder::default(),
        hidden: &|| vec!["SECRET_KEY".to_string()],
        stop: &|| false,
    };
    let command = r#"echo "$LLX_ONLY_GIVEN|${SECRET_KEY-none}|${HOME-none}""#;
    let report = tool.call(&json!({"command": command})).unwrap().report;
    // The process's own HOME is not in what was given: only what was given reaches it.
    assert!(report.ends_with("Output:\ngiven|none|none"), "{report}");
}

#[test]
fn a_source_file_of_a_thousand_lines_is_read_whole() {
    // About 10,000 tokens, what a model reads of one file at a time: no middle cut out.
    let dir = Scratch::new();
    let report = bash(
        &dir,
        &Recorder::default(),
        json!({"command": "awk 'BEGIN { for (i = 0; i < 1000; i++) printf \"line %04d of a file to read whole\\n\", i }'"}),
    )
    .unwrap();
    assert!(!report.contains(" cut ("), "{report}");
    assert!(
        report.contains("line 0500 of") && report.ends_with("line 0999 of a file to read whole")
    );
}

#[test]
fn a_cut_output_says_how_many_lines_it_had() {
    let dir = Scratch::new();
    let report = bash(
        &dir,
        &Recorder::default(),
        json!({"command": "awk 'BEGIN { for (i = 0; i < 20000; i++) printf \"line %05d\\n\", i }'"}),
    )
    .unwrap();
    assert!(
        report.starts_with("Exit code: 0\nWall time: ")
            && report.contains("\nTotal output lines: 20000\nOutput:\nline 00000"),
        "{}",
        report
    );
    assert!(report.contains(" cut ("), "{report}");
}

#[test]
fn an_escalated_command_asks_to_run_outside_the_sandbox() {
    let dir = Scratch::new();
    let gate = Recorder::default();
    bash(&dir, &gate, json!({"command": "true"})).unwrap();
    bash(&dir, &gate, json!({"command": "true", "escalate": true})).unwrap();
    assert_eq!(
        *gate.asked.lock().unwrap(),
        ["bash: true", "bash: true  (outside the sandbox) [open]"]
    );
}

#[cfg(target_os = "macos")]
#[test]
fn a_command_the_workspace_sandbox_refused_is_told_how_to_ask_for_more() {
    let dir = Scratch::new();
    let elsewhere = Scratch::new();
    let gate = Recorder::answering(contracts::gate::Verdict::Workspace);
    let report = bash(&dir, &gate, json!({"command": "echo made > made.txt"})).unwrap();
    assert!(report.starts_with("Exit code: 0"), "{report}");
    assert!(!report.contains("escalate"), "{report}");
    let outside = elsewhere.path().join("out.txt");
    let command = format!("echo x > '{}'", outside.display());
    let report = bash(&dir, &gate, json!({ "command": command })).unwrap();
    assert!(!outside.exists());
    assert!(report.contains("`escalate`"), "{report}");
}
