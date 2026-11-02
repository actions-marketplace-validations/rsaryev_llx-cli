use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::*;
use tools::shell::Scope;

fn run(command: &str, seconds: u64, stopped: &dyn Fn() -> bool) -> (Vec<Vec<String>>, String) {
    with((command, Scope::Open), &here(), &[], seconds, stopped)
}

fn hiding(
    command: &str,
    hidden: &[String],
    seconds: u64,
    stopped: &dyn Fn() -> bool,
) -> (Vec<Vec<String>>, String) {
    with((command, Scope::Open), &here(), hidden, seconds, stopped)
}

/// A workspace with the process's environment.
fn here() -> Workspace {
    Workspace::new(std::env::temp_dir()).unwrap()
}

fn with(
    (command, scope): (&str, Scope),
    workspace: &Workspace,
    hidden: &[String],
    seconds: u64,
    stopped: &dyn Fn() -> bool,
) -> (Vec<Vec<String>>, String) {
    let batches = Mutex::new(vec![]);
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let ended = watch(
        command,
        scope,
        workspace,
        hidden,
        deadline,
        stopped,
        &|lines, _| {
            batches.lock().unwrap().push(lines);
        },
    )
    .unwrap();
    let ended = match ended {
        Ended::Exit(how) => how,
        Ended::Deadline => "deadline".into(),
        Ended::Stopped => "stopped".into(),
    };
    (batches.into_inner().unwrap(), ended)
}

#[test]
fn lines_that_come_together_are_one_batch_and_the_exit_is_told() {
    // The first two in one write; the third only once their batch was given, so past the
    // gather window on any machine, and no later (R9).
    let scratch = crate::testing::Scratch::new();
    let workspace = Workspace::new(scratch.path()).unwrap();
    let go = scratch.path().join("go");
    let command = "printf 'one\\ntwo\\n'; until [ -e go ]; do sleep 0.02; done; echo three; exit 3";
    let batches = Mutex::new(vec![]);
    let deadline = Instant::now() + Duration::from_secs(10);
    let given = |lines: Vec<String>, _| {
        batches.lock().unwrap().push(lines);
        std::fs::write(&go, "").unwrap();
    };
    let ended = watch(
        command,
        Scope::Open,
        &workspace,
        &[],
        deadline,
        &|| false,
        &given,
    )
    .unwrap();
    assert!(matches!(&ended, Ended::Exit(how) if how == "exit 3"));
    let batches = batches.into_inner().unwrap();
    assert_eq!(batches, [vec!["one", "two"], vec!["three"]]);
}

#[test]
fn the_deadline_and_a_stop_end_the_command_and_what_it_started() {
    let (_, ended) = run("sleep 30 & sleep 30", 1, &|| false);
    assert_eq!(ended, "deadline");
    let begun = Instant::now();
    let (_, ended) = run("sleep 30", 30, &|| {
        begun.elapsed() > Duration::from_millis(300)
    });
    assert_eq!(ended, "stopped");
    assert!(begun.elapsed() < Duration::from_secs(5));
}

#[test]
fn the_apps_secrets_are_not_in_its_environment() {
    // A variable this process already has stands for a secret, so no test changes the
    // environment that the others share.
    let (name, _) = std::env::vars()
        .find(|(name, _)| {
            name != "PATH"
                && name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .expect("the test process has an environment variable");
    let command = format!("echo \"[${{{name}+set}}]\"");
    let (shown, _) = hiding(&command, &[], 10, &|| false);
    assert_eq!(shown, [vec!["[set]"]], "{name} is passed on unless hidden");
    let (hidden, _) = hiding(&command, std::slice::from_ref(&name), 10, &|| false);
    assert_eq!(hidden, [vec!["[]"]], "{name} was hidden");
}

#[test]
fn a_monitor_runs_with_the_environment_of_the_client() {
    let vars = [
        (
            "PATH".to_string(),
            std::env::var("PATH").unwrap_or_default(),
        ),
        ("LLX_PLACE".to_string(), "client".to_string()),
    ];
    let client = here().with_env(vars.to_vec());
    let command = r#"echo "$LLX_PLACE ${HOME-none}""#;
    let (lines, _) = with((command, Scope::Open), &client, &[], 10, &|| false);
    assert_eq!(lines, [vec!["client none"]]);
}

/// A confined monitor writes its `$TMPDIR`, as the mode's context tells the model, whatever
/// `TMPDIR` the client has: the sandbox lets it write only its scratch directory there.
#[cfg(target_os = "macos")]
#[test]
fn a_confined_monitor_has_a_scratch_tmpdir_of_its_own() {
    let vars = [
        (
            "PATH".to_string(),
            std::env::var("PATH").unwrap_or_default(),
        ),
        ("TMPDIR".to_string(), "/nowhere".to_string()),
    ];
    let client = here().with_env(vars.to_vec());
    let command = r#"echo kept > "$TMPDIR/x" && cat "$TMPDIR/x""#;
    for scope in [Scope::Workspace, Scope::ReadOnly] {
        let (lines, ended) = with((command, scope), &client, &[], 10, &|| false);
        assert_eq!(lines, [vec!["kept"]], "{scope:?}");
        assert_eq!(ended, "exit 0", "{scope:?}");
    }
}

/// A descriptor this process has open without close-on-exec when a monitor starts (one another
/// thread is making, before it marks it close-on-exec) never reaches it: a monitor lives long,
/// and what it holds stays open as long.
#[test]
fn a_monitor_inherits_only_its_output() {
    use std::os::fd::FromRawFd;
    // SAFETY: F_DUPFD makes a new descriptor for stderr, at 200 or above, without close-on-exec.
    let fd = unsafe { libc::fcntl(2, libc::F_DUPFD, 200) };
    assert!(fd >= 200, "{}", std::io::Error::last_os_error());
    // SAFETY: fcntl just made it, and nothing else owns it.
    let _stray = unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) };
    let probe = format!("[ -e /dev/fd/{fd} ] && echo inherited || echo closed");
    let (lines, _) = run(&probe, 10, &|| false);
    assert_eq!(lines, [vec!["closed"]]);
}

#[test]
fn a_command_that_closes_its_output_still_ends_at_its_deadline_and_its_stop() {
    let begun = Instant::now();
    let (_, ended) = run("exec >/dev/null 2>&1; sleep 30", 1, &|| false);
    assert_eq!(ended, "deadline");
    assert!(begun.elapsed() < Duration::from_secs(5));
    let begun = Instant::now();
    let (_, ended) = run("exec >/dev/null 2>&1; sleep 30", 30, &|| {
        begun.elapsed() > Duration::from_millis(300)
    });
    assert_eq!(ended, "stopped");
    assert!(begun.elapsed() < Duration::from_secs(5));
}
