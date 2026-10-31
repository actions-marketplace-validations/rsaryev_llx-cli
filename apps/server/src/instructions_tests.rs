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
