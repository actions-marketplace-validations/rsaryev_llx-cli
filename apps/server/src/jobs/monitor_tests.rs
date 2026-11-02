use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::*;
use tools::shell::Scope;

/// A workspace with the process's environment.
fn here() -> Workspace {
    Workspace::new(std::env::temp_dir()).unwrap()
}

/// What a wait of `command` checked every `every`, for `seconds` at most, came to: its end, and
/// each check as `seen` was given it.
fn wait_for(
    (command, scope): (&str, Scope),
    (workspace, hidden): (&Workspace, &[String]),
    (every, seconds): (Duration, u64),
    stopped: &dyn Fn() -> bool,
) -> (Waited, Vec<Check>) {
    let seen = Mutex::new(vec![]);
    let watch = Watch {
        command,
        scope,
        workspace,
        hidden,
        every,
        deadline: Instant::now() + Duration::from_secs(seconds),
    };
    let waited = wait(&watch, stopped, &|count, check| {
        let mut seen = seen.lock().unwrap();
        assert_eq!(count, seen.len() + 1, "each check counted once");
        seen.push(check.clone());
    });
    (waited, seen.into_inner().unwrap())
}

fn quick(command: &str, seconds: u64, stopped: &dyn Fn() -> bool) -> (Waited, Vec<Check>) {
    let every = Duration::from_millis(50);
    wait_for(
        (command, Scope::Open),
        (&here(), &[]),
        (every, seconds),
        stopped,
    )
}

/// What the last check printed.
fn printed(waited: &Waited) -> &str {
    waited
        .last
        .as_ref()
        .map_or("", |check| check.output.as_str())
}

#[test]
fn a_check_runs_again_until_it_exits_0_and_the_wait_ends_once_with_what_it_printed() {
    let scratch = crate::testing::Scratch::new();
    let workspace = Workspace::new(scratch.path()).unwrap();
    let ready = scratch.path().join("ready");
    let command = "if [ -e ready ]; then echo up; else echo 'not yet' >&2; exit 1; fi";
    let watch = Watch {
        command,
        scope: Scope::Open,
        workspace: &workspace,
        hidden: &[],
        every: Duration::from_millis(50),
        deadline: Instant::now() + Duration::from_secs(10),
    };
    // The condition holds once the second check ran.
    let waited = wait(&watch, &|| false, &|count, _| {
        if count == 2 {
            std::fs::write(&ready, "").unwrap();
        }
    });
    assert_eq!(waited.ended, Ended::Met);
    assert_eq!(waited.checks, 3);
    let last = waited.last.unwrap();
    assert_eq!(
        (last.how.as_str(), last.output.as_str()),
        ("Exit code: 0", "up")
    );
}

#[test]
fn a_check_that_fails_is_told_with_its_exit_and_its_errors() {
    let (waited, seen) = quick("echo 'connection refused' >&2; exit 7", 1, &|| false);
    assert_eq!(waited.ended, Ended::TimedOut);
    assert!(
        seen.len() >= 2,
        "it ran again until its time was up: {seen:?}"
    );
    let last = waited.last.unwrap();
    assert_eq!(
        (last.how.as_str(), last.output.as_str()),
        ("Exit code: 7", "connection refused")
    );
}

#[test]
fn a_check_that_cannot_run_ends_the_wait_at_once() {
    let begun = Instant::now();
    let (waited, seen) = quick("no-such-program-llx", 30, &|| false);
    assert_eq!(waited.ended, Ended::Unrunnable);
    assert_eq!(seen.len(), 1);
    assert!(
        printed(&waited).contains("not found"),
        "{}",
        printed(&waited)
    );
    assert!(begun.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_stop_ends_the_wait_between_checks_and_during_one() {
    for command in ["exit 1", "sleep 30 & sleep 30"] {
        let begun = Instant::now();
        let stopped = || begun.elapsed() > Duration::from_millis(300);
        let every = Duration::from_secs(20);
        let (waited, _) = wait_for(
            (command, Scope::Open),
            (&here(), &[]),
            (every, 60),
            &stopped,
        );
        assert_eq!(waited.ended, Ended::Stopped, "{command}");
        assert!(begun.elapsed() < Duration::from_secs(5), "{command}");
    }
}

#[test]
fn a_check_longer_than_its_interval_is_killed_there_with_what_it_started() {
    let scratch = crate::testing::Scratch::new();
    let pid = scratch.path().join("pid");
    let command = format!("sleep 30 & echo $! > {}; wait", pid.display());
    let every = Duration::from_millis(300);
    let (waited, seen) = wait_for((&command, Scope::Open), (&here(), &[]), (every, 1), &|| {
        false
    });
    assert_eq!(waited.ended, Ended::TimedOut);
    assert!(seen[0].how.starts_with("Timed out after"), "{:?}", seen[0]);
    let pid: i32 = std::fs::read_to_string(&pid)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // SAFETY: signal 0 only asks whether the process exists.
    let alive = unsafe { libc::kill(pid, 0) } == 0;
    assert!(!alive, "what the check started outlived it");
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
    let every = (Duration::from_millis(50), 10);
    let (shown, _) = wait_for((&command, Scope::Open), (&here(), &[]), every, &|| false);
    assert_eq!(
        printed(&shown),
        "[set]",
        "{name} is passed on unless hidden"
    );
    let hiding = std::slice::from_ref(&name);
    let (hidden, _) = wait_for((&command, Scope::Open), (&here(), hiding), every, &|| false);
    assert_eq!(printed(&hidden), "[]", "{name} was hidden");
}

#[test]
fn a_monitor_runs_with_the_environment_it_is_given() {
    let vars = [
        (
            "PATH".to_string(),
            std::env::var("PATH").unwrap_or_default(),
        ),
        ("LLX_GIVEN".to_string(), "given".to_string()),
    ];
    let given = here().with_env(vars.to_vec());
    let command = r#"echo "$LLX_GIVEN ${HOME-none}""#;
    let every = (Duration::from_millis(50), 10);
    let (waited, _) = wait_for((command, Scope::Open), (&given, &[]), every, &|| false);
    assert_eq!(printed(&waited), "given none");
}

/// A confined monitor writes its `$TMPDIR`, as the mode's context tells the model, whatever
/// `TMPDIR` its environment has: the sandbox lets it write only its scratch directory there.
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
    let given = here().with_env(vars.to_vec());
    let command = r#"echo kept > "$TMPDIR/x" && cat "$TMPDIR/x""#;
    for scope in [Scope::Workspace, Scope::ReadOnly] {
        let every = (Duration::from_millis(50), 10);
        let (waited, _) = wait_for((command, scope), (&given, &[]), every, &|| false);
        assert_eq!(printed(&waited), "kept", "{scope:?}");
        assert_eq!(waited.ended, Ended::Met, "{scope:?}");
    }
}

/// A descriptor this process has open without close-on-exec when a check starts (one another
/// thread is making, before it marks it close-on-exec) never reaches it.
#[test]
fn a_check_inherits_only_its_output() {
    use std::os::fd::FromRawFd;
    // SAFETY: F_DUPFD makes a new descriptor for stderr, at 200 or above, without close-on-exec.
    let fd = unsafe { libc::fcntl(2, libc::F_DUPFD, 200) };
    assert!(fd >= 200, "{}", std::io::Error::last_os_error());
    // SAFETY: fcntl just made it, and nothing else owns it.
    let _stray = unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) };
    let probe = format!("[ -e /dev/fd/{fd} ] && echo inherited || echo closed");
    let (waited, _) = quick(&probe, 10, &|| false);
    assert_eq!(printed(&waited), "closed");
}

#[test]
fn a_check_that_closes_its_output_still_ends_at_its_limit_and_its_stop() {
    let begun = Instant::now();
    let every = Duration::from_millis(300);
    let quiet = ("exec >/dev/null 2>&1; sleep 30", Scope::Open);
    let (waited, _) = wait_for(quiet, (&here(), &[]), (every, 1), &|| false);
    assert_eq!(waited.ended, Ended::TimedOut);
    assert!(begun.elapsed() < Duration::from_secs(5));
    let begun = Instant::now();
    let stopped = || begun.elapsed() > Duration::from_millis(300);
    let every = Duration::from_secs(20);
    let (waited, _) = wait_for(quiet, (&here(), &[]), (every, 30), &stopped);
    assert_eq!(waited.ended, Ended::Stopped);
    assert!(begun.elapsed() < Duration::from_secs(5));
}
