//! `llx grep` as an agent runs it: the files that answer a question, with their lines; picked by
//! the decision model where one is there, else ranked by the question's words.

use std::path::Path;

use serde_json::Value;

use crate::common::home::{Home, output, text};
use crate::common::model::{Model, Reply};

/// A small project in `home`: what retries a request, what only sends one, and notes.
fn project(home: &Home) -> &Path {
    let files = [
        (
            "project/src/retry.rs",
            "//! Sending again.\n\n/// Send the request again after a failure, three times at most.\npub fn retry(request: &Request) -> Response {\n    for _ in 0..3 {\n        if let Ok(response) = send(request) {\n            return response;\n        }\n    }\n    fail()\n}\n",
        ),
        (
            "project/src/http.rs",
            "//! The HTTP client.\n\npub fn send(request: &Request) -> Result<Response, Error> {\n    post(request)\n}\n",
        ),
        ("project/docs/notes.md", "# Notes\n\nNothing about it.\n"),
    ];
    for (name, content) in files {
        let path = home.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    home.path()
}

fn grep(home: &Home, model: Option<&Model>, args: &[&str]) -> std::process::Output {
    let mut command = match model {
        Some(model) => home.with_model(model, args),
        None => home.llx(args),
    };
    output(command.current_dir(home.path()))
}

#[test]
fn without_a_decision_model_the_files_are_ranked_by_the_question_s_words() {
    let home = Home::new();
    project(&home);
    let out = grep(
        &home,
        None,
        &["grep", "where is a failed request sent again", "project"],
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let shown = text(&out.stdout);
    let first = shown.lines().next().unwrap();
    assert!(first.starts_with("project/src/retry.rs  1.00"), "{shown}");
    // The function whole, with its comment, line-numbered.
    assert!(
        shown.contains(" 3: /// Send the request again after a failure"),
        "{shown}"
    );
    assert!(shown.contains("11: }"), "{shown}");
    assert!(shown.find("project/src/http.rs").unwrap() > shown.find("retry.rs").unwrap());
    let said = text(&out.stderr);
    assert!(
        said.contains("by their words alone") && said.contains("OpenRouter"),
        "{said}"
    );
}

#[test]
fn the_decision_model_reads_the_best_files_and_keeps_those_that_answer() {
    let home = Home::new();
    project(&home);
    let model = Model::start(vec![Reply::decided("yes", &[("yes", 0.9), ("no", 0.1)])]);
    let args = [
        "grep",
        "where is a failed request sent again",
        "project",
        "--judge",
        "1",
        "--files",
    ];
    let out = grep(&home, Some(&model), &args);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    // One file read: the best by its words, and it answers.
    assert_eq!(text(&out.stdout), "project/src/retry.rs\n");
    let asked = model.advised();
    assert_eq!(asked.len(), 1);
    let question = asked[0]["questions"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert!(
        question["instructions"]
            .to_string()
            .contains("where is a failed request sent again")
    );
    assert_eq!(asked[0]["state"]["path"], "project/src/retry.rs");
    assert!(
        asked[0]["state"]["text"]
            .as_str()
            .unwrap()
            .contains("pub fn retry")
    );
    assert!(
        text(&out.stderr).contains("1 of 3 files answer, 1 read by"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn when_the_model_says_no_file_answers_nothing_is_shown_and_the_exit_is_1() {
    let home = Home::new();
    project(&home);
    let no = || Reply::decided("no", &[("yes", 0.05), ("no", 0.95)]);
    let model = Model::start(vec![no(), no(), no()]);
    let out = grep(
        &home,
        Some(&model),
        &["grep", "where is the GraphQL schema", "project"],
    );
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "");
    // Every file read: there are few.
    assert_eq!(model.advised().len(), 3);
    assert!(
        text(&out.stderr).contains("0 of 3 files answer"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn json_gives_each_file_with_its_score_and_lines() {
    let home = Home::new();
    project(&home);
    let out = grep(
        &home,
        None,
        &["grep", "retry", "project", "--json", "--lexical"],
    );
    let answer: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(answer["by"], "words");
    assert_eq!(answer["files"][0]["file"], "project/src/retry.rs");
    assert_eq!(answer["files"][0]["blocks"][0]["start"], 3);
    assert!(
        answer["files"][0]["blocks"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("/// Send")
    );
    // --lexical asks no model, even with a key there.
    assert!(!text(&out.stderr).contains("OpenRouter"));
}

#[test]
fn in_git_what_git_ignores_is_not_read() {
    let home = Home::new();
    let root = project(&home).join("project");
    std::fs::write(root.join(".gitignore"), "src/http.rs\n").unwrap();
    let git = |args: &[&str]| {
        let done = output(
            std::process::Command::new("git")
                .args(args)
                .current_dir(&root),
        );
        assert!(done.status.success(), "{}", text(&done.stderr));
    };
    git(&["init", "-q"]);
    let out = grep(&home, None, &["grep", "send request", "project", "--files"]);
    assert_eq!(
        text(&out.stdout),
        "project/src/retry.rs\n",
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn a_question_with_no_word_found_finds_nothing_and_a_path_not_there_is_named() {
    let home = Home::new();
    project(&home);
    let out = grep(&home, None, &["grep", "kubernetes", "project"]);
    assert_eq!(out.status.code(), Some(1));
    let out = grep(&home, None, &["grep", "retry", "gone"]);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("gone"));
}
