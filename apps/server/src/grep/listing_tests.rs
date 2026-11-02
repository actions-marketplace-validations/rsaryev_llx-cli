use std::path::Path;

use super::{listed, read};
use crate::testing::Scratch;

#[test]
fn outside_git_every_file_is_read_but_hidden_ones_build_output_and_lock_files() {
    let Some(scratch) = Scratch::outside_repository() else {
        return;
    };
    for name in [
        "src/a.rs",
        "src/deep/b.rs",
        "README.md",
        ".env",
        ".git/config",
        "target/x.rs",
        "node_modules/m/i.js",
        "Cargo.lock",
    ] {
        scratch.file(name, "text\n");
    }
    let root = scratch.path().display().to_string();
    let found: Vec<String> = listed(Path::new("."), &[scratch.path().to_path_buf()]).unwrap();
    let found: Vec<&str> = found
        .iter()
        .map(|file| file.strip_prefix(&root).unwrap())
        .collect();
    assert_eq!(found, ["/README.md", "/src/a.rs", "/src/deep/b.rs"]);
}

#[test]
fn a_file_given_is_read_and_a_path_that_is_not_there_is_named() {
    let scratch = Scratch::new();
    let file = scratch.file("one.rs", "fn one() {}\n");
    assert_eq!(
        listed(Path::new("."), std::slice::from_ref(&file)).unwrap(),
        [file.display().to_string()]
    );
    let gone = scratch.path().join("gone");
    let failure = listed(Path::new("."), &[gone]).err().unwrap();
    assert!(failure.message().unwrap().contains("gone"), "{failure:?}");
}

#[test]
fn only_text_is_read() {
    let scratch = Scratch::new();
    let text = scratch.file("t.rs", "fn t() {}\n");
    assert_eq!(
        read(Path::new("."), &text.display().to_string())
            .unwrap()
            .text,
        "fn t() {}\n"
    );
    let binary = scratch.path().join("b.bin");
    std::fs::write(&binary, b"\x7fELF\0\0\x01").unwrap();
    assert!(read(Path::new("."), &binary.display().to_string()).is_none());
    let large = scratch.path().join("large.txt");
    std::fs::write(&large, "x".repeat((1 << 20) + 1)).unwrap();
    assert!(read(Path::new("."), &large.display().to_string()).is_none());
}
