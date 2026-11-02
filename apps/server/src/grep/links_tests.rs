use super::{around, linked, modules};
use crate::grep::listing::Doc;

fn doc(path: &str, text: &str) -> Doc {
    Doc {
        path: path.into(),
        text: text.into(),
    }
}

fn names(text: &str) -> Vec<String> {
    modules(text).into_iter().collect()
}

#[test]
fn a_paths_prefixes_are_its_modules_and_a_use_list_names_its_own() {
    assert_eq!(
        names("let out = tools::shell::query(command, wait);"),
        ["tools", "tools::shell"]
    );
    assert_eq!(
        names("use std::process::{Command, Stdio};"),
        ["std", "std::process"]
    );
    // Where a path is, not what: `crate::` and `self::` lead nothing.
    assert_eq!(names("crate::error::Failure"), ["error"]);
}

#[test]
fn what_other_languages_import_is_a_module_too() {
    assert_eq!(names("import subprocess"), ["subprocess"]);
    assert_eq!(names("from os.path import join"), ["os.path"]);
    assert_eq!(
        names("import { spawn } from \"node:child_process\";"),
        ["node:child_process"]
    );
    assert_eq!(
        names("const cp = require('child_process');"),
        ["child_process"]
    );
    // Prose says "from" too.
    assert!(names("// the key comes from the person").is_empty());
}

#[test]
fn the_files_naming_a_kept_files_rarer_modules_are_reached_and_those_already_read_are_not() {
    let mut docs = vec![
        doc("monitor.rs", "tools::shell::scoped(x)\nstd::fmt::Write\n"),
        doc("doctor.rs", "tools::shell::query(x)\n"),
        doc("extras.rs", "use tools::shell::Unanswered;\n"),
        doc("read.rs", "tools::shell::query(y)\n"),
        doc("tested.rs", "tools::shell::query(z)\n"),
    ];
    // `std::fmt` is named by most files: it links nothing.
    for n in 0..16 {
        docs.push(doc(&format!("other{n}.rs"), "std::fmt::Display\n"));
    }
    let read: Vec<bool> = (0..docs.len()).map(|doc| doc == 0 || doc == 3).collect();
    let reached = linked(&docs, &[0], &read, |doc| doc != 4);
    let paths: Vec<&str> = reached
        .iter()
        .map(|(doc, _)| docs[*doc].path.as_str())
        .collect();
    assert_eq!(paths, ["doctor.rs", "extras.rs"]);
    assert!(
        reached[0].1.contains(&"tools::shell".to_string()),
        "{:?}",
        reached[0].1
    );
}

#[test]
fn a_reached_file_is_shown_around_the_lines_that_name_the_link() {
    let text = (1..=40)
        .map(|n| match n {
            20 => "    match tools::shell::query(cmd, WAIT) {".to_string(),
            n => format!("line {n}"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let parts = around(&text, &["tools::shell".into()], 60);
    assert_eq!(parts.spans, [(16, 28)]);
    assert!(
        parts.text.starts_with("[part 1: lines 17-28]\nline 17\n"),
        "{}",
        parts.text
    );
    assert!(parts.text.contains("tools::shell::query"));
    // Within its lines.
    assert_eq!(around(&text, &["tools::shell".into()], 5).spans, [(16, 21)]);
}
