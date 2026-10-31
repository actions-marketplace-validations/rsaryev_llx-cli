//! `--allow` and `--deny` as a pipeline uses them: with no one at a terminal, what an allow rule
//! names runs (and only what it names), what a deny rule names is refused in every mode, and the
//! model is told which commands it may run.

use serde_json::json;

use crate::common::home::{Home, text};
use crate::common::model::{Model, Reply, running, said};

/// [`running`], the command asked to leave the sandbox: what the default mode does not settle by
/// itself, so a rule decides it.
fn escalating(command: &str) -> Model {
    Model::start(vec![
        Reply::call("bash", json!({"command": command, "escalate": true})),
        Reply::done("Done."),
    ])
}

#[test]
fn plan_mode_runs_an_allowed_command_and_tells_the_model_it_may() {
    let (home, model) = (Home::new(), running("echo checked"));
    let args = [
        "exec",
        "--mode",
        "plan",
        "--allow",
        "bash(echo *)",
        "check it",
    ];
    let out = home.exec(&model, &args, "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let first = said(&model, 0);
    assert!(
        first.contains("bash(echo *)"),
        "the context names the rule: {first}"
    );
    let told = said(&model, 1);
    assert!(
        told.contains("checked") && !told.contains("not allowed"),
        "{told}"
    );
}

#[test]
fn an_allowed_command_chained_to_another_is_refused() {
    let home = Home::new();
    for command in [
        "echo hi && touch made.txt",
        "echo hi; touch made.txt",
        "echo $(touch made.txt)",
        "echo hi > made.txt",
    ] {
        let model = escalating(command);
        let args = ["exec", "--allow", "bash(echo *)", "x"];
        let out = home.exec(&model, &args, "");
        assert_eq!(
            out.status.code(),
            Some(0),
            "{command}: {}",
            text(&out.stderr)
        );
        assert!(!home.path().join("made.txt").exists(), "{command} ran");
        assert!(said(&model, 1).contains("not allowed"), "{command}");
    }
}

/// A repository in the home whose `origin` is a bare one beside it, and `receive.sh`, a program a
/// push can be told to run on the other side, which leaves `made.txt` where it ran.
fn repository_with_a_program(home: &Home) {
    let made = home.path().join("made.txt");
    let script = format!(
        "git init -q --bare remote.git && git init -q . && git remote add origin remote.git \
         && git -c user.name=t -c user.email=t@t commit -q --allow-empty -m x \
         && printf '#!/bin/sh\\ntouch {}\\nexec git-receive-pack \"$@\"\\n' > receive.sh \
         && chmod +x receive.sh",
        made.display()
    );
    let mut setup = home.command("sh");
    let status = setup
        .arg("-c")
        .arg(script)
        .current_dir(home.path())
        .spawn()
        .unwrap()
        .wait()
        .unwrap();
    assert!(status.success(), "the repository could not be set up");
}

#[test]
fn an_allowed_git_push_cannot_be_told_a_program_to_run() {
    let home = Home::new();
    repository_with_a_program(&home);
    let model = escalating("git push --receive-pack=./receive.sh origin HEAD:refs/heads/x");
    let args = ["exec", "--allow", "bash(git push *)", "x"];
    let out = home.exec(&model, &args, "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(!home.path().join("made.txt").exists(), "the program ran");
    assert!(
        said(&model, 1).contains("not allowed"),
        "{}",
        said(&model, 1)
    );
}

#[test]
fn an_allowed_git_push_still_pushes() {
    let home = Home::new();
    repository_with_a_program(&home);
    let model = escalating("git push -u origin HEAD:refs/heads/x");
    let args = ["exec", "--allow", "bash(git push *)", "x"];
    let out = home.exec(&model, &args, "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(home.path().join("remote.git/refs/heads/x").exists());
    assert!(
        !said(&model, 1).contains("not allowed"),
        "{}",
        said(&model, 1)
    );
}

#[test]
fn a_deny_rule_refuses_even_with_yes_and_names_itself() {
    let (home, model) = (Home::new(), running("touch made.txt"));
    let args = ["exec", "-y", "--deny", "bash(touch *)", "x"];
    let out = home.exec(&model, &args, "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(!home.path().join("made.txt").exists());
    let told = said(&model, 1);
    assert!(told.contains("bash(touch *)"), "{told}");
}

#[test]
fn an_apply_patch_rule_allows_the_paths_it_names_and_no_others() {
    let home = Home::new();
    // A mode that reaches anything, so nothing but the rule allows a change.
    let dir = home.path().join("config/llx");
    std::fs::create_dir_all(&dir).unwrap();
    let modes = json!({"modes": {"careful": {"reach": "anything"}}});
    std::fs::write(dir.join("settings.json"), modes.to_string()).unwrap();
    let model = Model::start(vec![
        Reply::create("docs/a.md", "a"),
        Reply::create("src/b.rs", "b"),
        Reply::done("Done."),
    ]);
    let args = [
        "exec",
        "--mode",
        "careful",
        "--allow",
        "apply_patch(docs/**)",
        "x",
    ];
    let out = home.exec(&model, &args, "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(home.path().join("docs/a.md").exists());
    assert!(!home.path().join("src/b.rs").exists());
}

#[test]
fn yes_lets_a_patch_change_files() {
    let home = Home::new();
    let model = Model::start(vec![Reply::create("a.md", "a"), Reply::done("Done.")]);
    let out = home.exec(&model, &["exec", "-y", "x"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(home.path().join("a.md").exists(), "{}", said(&model, 1));
}

#[test]
fn plan_mode_refuses_a_patch_whatever_is_allowed() {
    let home = Home::new();
    let model = Model::start(vec![Reply::create("a.md", "a"), Reply::done("Done.")]);
    let args = ["exec", "--mode", "plan", "--allow", "apply_patch", "x"];
    let out = home.exec(&model, &args, "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(!home.path().join("a.md").exists());
    let told = said(&model, 1);
    assert!(told.contains("read-only"), "{told}");
}

#[test]
fn plan_mode_stays_read_only_whatever_is_allowed() {
    let (home, model) = (Home::new(), running("echo a > a.md"));
    let args = ["exec", "--mode", "plan", "--allow", "bash", "x"];
    let out = home.exec(&model, &args, "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(!home.path().join("a.md").exists());
}

#[test]
fn a_rule_that_cannot_be_read_is_not_ready() {
    let home = Home::new();
    for rule in ["bash(echo *", "nosuchtool", "apply_patch(//etc/**)"] {
        let model = running("echo hi");
        let out = home.exec(&model, &["exec", "--allow", rule, "x"], "");
        assert_eq!(out.status.code(), Some(3), "{rule}");
        assert!(
            text(&out.stderr).contains(rule),
            "{rule}: {}",
            text(&out.stderr)
        );
    }
}
