use std::time::Duration;

use super::Scope;
use crate::shell::process::{Ending, Finished, Job, run};
use crate::testing::Scratch;
use crate::workspace::Workspace;

fn read_only(command: &str, dir: &std::path::Path) -> Finished {
    run(&Job {
        command,
        dir,
        timeout: Duration::from_secs(60),
        hidden: &[],
        stop: &|| false,
        scope: Scope::ReadOnly,
        workspace: &Workspace::new(dir).unwrap(),
    })
    .unwrap()
}

/// The system's word for a refusal: macOS's EPERM, Linux's EACCES (Landlock) or EPERM (seccomp).
fn refused(stderr: &str) -> bool {
    stderr.contains("not permitted") || stderr.contains("Permission denied")
}

#[test]
fn a_read_only_command_reads_and_runs_but_changes_nothing() {
    let dir = Scratch::new();
    let notes = dir.file("notes.txt", "kept");
    let finished = read_only(
        "cat notes.txt; echo changed > notes.txt; rm notes.txt; mkdir made; touch new.txt; \
         mv notes.txt moved.txt; ln -s notes.txt link; echo done",
        dir.path(),
    );
    assert!(finished.output.starts_with("kept"), "{}", finished.output);
    assert!(finished.output.ends_with("done"), "{}", finished.output);
    assert!(refused(&finished.output), "{}", finished.output);
    assert_eq!(std::fs::read_to_string(&notes).unwrap(), "kept");
    let left: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().collect();
    assert_eq!(left.len(), 1, "{left:?}");
}

#[test]
fn a_read_only_command_writes_nowhere_outside_either() {
    let dir = Scratch::new();
    let elsewhere = Scratch::new();
    let target = elsewhere.path().join("out.txt");
    let finished = read_only(&format!("echo x > '{}'", target.display()), dir.path());
    assert_ne!(finished.ending, Ending::Exit(0));
    assert!(!target.exists());
}

#[test]
fn a_read_only_command_has_a_scratch_directory_of_its_own_and_dev_null() {
    let dir = Scratch::new();
    let finished = read_only(
        r#"echo kept > "$TMPDIR/x" && cat "$TMPDIR/x" && echo gone > /dev/null && echo "$TMPDIR""#,
        dir.path(),
    );
    assert_eq!(finished.ending, Ending::Exit(0), "{}", finished.output);
    let mut lines = finished.output.lines();
    assert_eq!(lines.next(), Some("kept"));
    let scratch = lines.next().unwrap_or_default();
    // Removed once the command ended.
    assert!(!std::path::Path::new(scratch).exists(), "{scratch}");
}

#[test]
fn the_scratch_directory_survives_a_clients_environment() {
    // The daemon runs a command with the client's variables, whose own `TMPDIR` is not writable
    // from the sandbox.
    let dir = Scratch::new();
    let vars = [
        ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        ("TMPDIR".to_string(), "/nowhere".to_string()),
    ];
    let finished = run(&Job {
        command: r#"echo kept > "$TMPDIR/x" && cat "$TMPDIR/x""#,
        dir: dir.path(),
        timeout: Duration::from_secs(60),
        hidden: &[],
        stop: &|| false,
        scope: Scope::ReadOnly,
        workspace: &Workspace::new(dir.path()).unwrap().with_env(vars.to_vec()),
    })
    .unwrap();
    assert_eq!(finished.ending, Ending::Exit(0), "{}", finished.output);
    assert_eq!(finished.output, "kept");
}

#[test]
fn a_read_only_command_opens_no_socket() {
    let dir = Scratch::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let finished = read_only(
        &format!(
            "python3 -c \"import socket; socket.create_connection(('127.0.0.1', {port}))\" \
             && echo connected"
        ),
        dir.path(),
    );
    assert!(
        !finished.output.contains("connected"),
        "{}",
        finished.output
    );
    assert!(refused(&finished.output), "{}", finished.output);
}

#[test]
fn an_open_command_changes_what_it_says() {
    let dir = Scratch::new();
    let finished = run(&Job {
        command: "echo made > new.txt",
        dir: dir.path(),
        timeout: Duration::from_secs(60),
        hidden: &[],
        stop: &|| false,
        scope: Scope::Open,
        workspace: &Workspace::new(dir.path()).unwrap(),
    })
    .unwrap();
    assert_eq!(finished.ending, Ending::Exit(0));
    assert!(dir.path().join("new.txt").exists());
}

/// What the app keeps to itself (its keys, its sessions) stays unread, however the command names
/// it; the rest of the same directory reads. Linux's Landlock cannot take a path out of what it
/// lets read: there the workspace's own check is all.
#[cfg(target_os = "macos")]
#[test]
fn a_read_only_command_reads_nothing_the_app_keeps_to_itself() {
    let dir = Scratch::new();
    dir.file("config/credentials", "secret");
    dir.file("notes.txt", "open");
    let kept = [dir.path().join("config")];
    let finished = run(&Job {
        command: "cat notes.txt; cat config/credentials; cat ./config/../config/credentials",
        dir: dir.path(),
        timeout: Duration::from_secs(60),
        hidden: &[],
        stop: &|| false,
        scope: Scope::ReadOnly,
        workspace: &Workspace::new(dir.path()).unwrap().denying(kept),
    })
    .unwrap();
    assert!(finished.output.starts_with("open"), "{}", finished.output);
    assert!(!finished.output.contains("secret"), "{}", finished.output);
    assert!(refused(&finished.output), "{}", finished.output);
}

fn in_workspace(command: &str, dir: &std::path::Path) -> Finished {
    run(&Job {
        command,
        dir,
        timeout: Duration::from_secs(60),
        hidden: &[],
        stop: &|| false,
        scope: Scope::Workspace,
        workspace: &Workspace::new(dir).unwrap(),
    })
    .unwrap()
}

/// The workspace's scope writes the workspace, but not its `.git`, where a hook would run
/// unconfined at the next commit, and nothing outside.
#[cfg(target_os = "macos")]
#[test]
fn a_workspace_command_changes_the_workspace_alone_and_never_its_git() {
    let dir = Scratch::new();
    let elsewhere = Scratch::new();
    dir.file(".git/config", "[core]\n");
    dir.file("src/old.txt", "old");
    let outside = elsewhere.path().join("out.txt");
    let finished = in_workspace(
        &format!(
            "echo made > new.txt && mkdir -p src/deep && mv src/old.txt src/deep/ && \
             echo t > \"$TMPDIR/t\" && echo ok; echo hook > .git/hooks-pre-commit; \
             echo x > '{}'; echo end",
            outside.display()
        ),
        dir.path(),
    );
    assert!(finished.output.starts_with("ok"), "{}", finished.output);
    assert!(refused(&finished.output), "{}", finished.output);
    assert!(dir.path().join("new.txt").exists());
    assert!(dir.path().join("src/deep/old.txt").exists());
    assert!(!dir.path().join(".git/hooks-pre-commit").exists());
    assert!(!outside.exists());
}

/// A nested repository's `.git` is kept as the workspace's own is, in any case and made anew: a
/// hook written there would run unconfined at the person's next commit in it. Names that only
/// start or end like it (`.gitignore`, `.github`) are the workspace's.
#[cfg(target_os = "macos")]
#[test]
fn a_workspace_command_never_writes_a_nested_repositorys_git() {
    let dir = Scratch::new();
    dir.file("sub/.git/config", "[core]\n");
    let finished = in_workspace(
        "echo hook > sub/.git/hooks-pre-commit; echo x > sub/.GIT/y; mkdir -p new/.git; \
         mkdir file && echo x > file/.git; echo x > .gitignore && mkdir .github && \
         echo x > .github/ci.yml && echo x > repo.git && echo kept",
        dir.path(),
    );
    assert!(finished.output.ends_with("kept"), "{}", finished.output);
    assert!(refused(&finished.output), "{}", finished.output);
    for written in [".gitignore", ".github/ci.yml", "repo.git"] {
        assert!(dir.path().join(written).exists(), "{written}");
    }
    for kept in [
        "sub/.git/hooks-pre-commit",
        "sub/.git/y",
        "new/.git",
        "file/.git",
    ] {
        assert!(
            !dir.path().join(kept).exists(),
            "{kept}: {}",
            finished.output
        );
    }
}

/// Landlock cannot take a path out of a tree it lets write, so no `.git` in the workspace (its
/// own, a nested repository's, or one made anew) could be kept from a command: on Linux the
/// workspace's scope only reads, a plain directory as a repository.
#[cfg(target_os = "linux")]
#[test]
fn a_workspace_command_on_linux_only_reads() {
    let repository = Scratch::new();
    repository.file(".git/config", "[core]\n");
    let nested = Scratch::new();
    nested.file("sub/.git/config", "[core]\n");
    let plain = Scratch::new();
    for (dir, written) in [
        (&repository, "new.txt"),
        (&nested, "sub/.git/hooks-pre-commit"),
        (&plain, "new.txt"),
        (&plain, ".git"),
    ] {
        let finished = in_workspace(&format!("echo made > {written}"), dir.path());
        assert!(refused(&finished.output), "{written}: {}", finished.output);
        assert!(!dir.path().join(written).exists(), "{written}");
    }
}
