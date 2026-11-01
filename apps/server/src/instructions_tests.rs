use super::*;
use crate::testing::Scratch;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn each_directory_gives_its_first_file_from_the_root_down() {
    let repo = Scratch::new();
    let root = repo.path();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(&root.join("AGENTS.md"), "root rules");
    write(&root.join("CLAUDE.md"), "@AGENTS.md");
    write(&root.join("app/CLAUDE.md"), "app rules");
    write(&root.join("app/web/AGENTS.md"), "web rules");
    write(&root.join("app/web/AGENTS.override.md"), "web override");
    let found = project(&root.join("app/web"));
    let texts: Vec<&str> = found.iter().map(|f| f.text.as_str()).collect();
    assert_eq!(texts, ["root rules", "app rules", "web override"]);
}

#[test]
fn a_line_that_is_only_an_at_path_brings_that_file_in_once() {
    let repo = Scratch::new();
    let root = repo.path();
    write(
        &root.join("CLAUDE.md"),
        "Read these:\n@docs/style.md\n@docs/style.md\n@missing.md\nand email me@example.com",
    );
    write(&root.join("docs/style.md"), "Use tabs.\n@../CLAUDE.md");
    let found = in_dir(root, FILE_LIMIT).unwrap();
    assert_eq!(
        found.text,
        "Read these:\nUse tabs.\n@missing.md\nand email me@example.com"
    );
}

#[test]
fn the_persons_own_file_lives_in_the_llx_home() {
    let home = Scratch::new();
    assert_eq!(personal(Some(home.path())), None);
    write(&home.path().join(".llx/AGENTS.md"), "Answer briefly.");
    assert_eq!(personal(Some(home.path())).unwrap().text, "Answer briefly.");
}

#[test]
fn a_huge_file_is_cut_and_says_so() {
    let repo = Scratch::new();
    write(&repo.path().join("AGENTS.md"), &"x".repeat(FILE_LIMIT + 10));
    let found = project(repo.path());
    assert!(
        found[0]
            .text
            .ends_with("[10 more bytes not shown: read the file for the rest]")
    );
}

fn call(name: &str, arguments: Value) -> ToolCall {
    ToolCall {
        id: "c1".into(),
        name: name.into(),
        arguments,
    }
}

#[test]
fn a_command_that_reads_an_instruction_file_is_kept() {
    let output = "Exit code: 0\nWall time: 0 seconds\nOutput:\nweb rules";
    for command in ["cat app/web/AGENTS.md", "sed -n 1,200p docs/CLAUDE.md"] {
        let read = call("bash", serde_json::json!({ "command": command }));
        assert!(reads_one(&read, output), "{command}");
    }
}

#[test]
fn another_command_another_tool_or_a_larger_output_is_not() {
    let output = "Exit code: 0\nOutput:\nfn main() {}";
    let other = call("bash", serde_json::json!({"command": "cat src/main.rs"}));
    assert!(!reads_one(&other, output));
    let patch = call(
        "apply_patch",
        serde_json::json!("*** Update File: AGENTS.md"),
    );
    assert!(!reads_one(&patch, output));
    // A search of every file that names one is no instruction file.
    let search = call("bash", serde_json::json!({"command": "rg -n AGENTS.md"}));
    assert!(!reads_one(&search, &"x".repeat(FILE_LIMIT * 2)));
}
