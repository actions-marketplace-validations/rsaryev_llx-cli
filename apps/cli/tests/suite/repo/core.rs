//! The core decides, the shell acts (ARCHITECTURE.md C1, C2, R9), checked on the sources: the
//! vocabulary and the core open no file, socket or process, read no environment, print nothing
//! and never sleep. Effects leave them through a port; time comes in as a value the caller gives.

use std::path::PathBuf;

use super::{root, shown, walk, working};

/// The crates that only decide: the ports and the agent's loop.
const CORE: [&str; 2] = ["packages/contracts", "packages/agent"];

/// What reaches the OS, as the source says it: each is the shell's, never the core's.
const EFFECTS: [&str; 13] = [
    "std::env",
    "std::fs",
    "std::process",
    "std::net",
    "io::stdout",
    "io::stderr",
    "io::stdin",
    "print!",
    "println!",
    "eprint!",
    "eprintln!",
    "dbg!",
    "thread::sleep",
];

/// The modules of `std` that the core never names, also when imported in a group
/// (`use std::{env, fs};`).
const GROUPED: [&str; 4] = ["env", "fs", "process", "net"];

/// The working code of the crate `dir`: its `src/`, without the tests beside it and the
/// test-only fakes.
fn working_in(dir: &str) -> Vec<PathBuf> {
    let mut files = vec![];
    walk(&root().join(dir).join("src"), &working, &mut files);
    files
}

/// The first effect `line` names: one of [`EFFECTS`] where a path or a macro starts (not inside
/// a longer name: `eprint!` is not `print!`), or a [`GROUPED`] module in a `use std::{…}`.
fn effect(line: &str) -> Option<&'static str> {
    let starts = |at: usize| !line[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_');
    let named = EFFECTS
        .iter()
        .copied()
        .find(|word| line.match_indices(word).any(|(at, _)| starts(at)));
    if named.is_some() {
        return named;
    }
    let group = line.trim_start().strip_prefix("use std::{")?;
    let words: Vec<&str> = group
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .collect();
    GROUPED
        .iter()
        .copied()
        .find(|module| words.contains(module))
}

/// Every effect named in the core's working code, as `file:line: effect in "code"`.
fn effects_in_the_core() -> Vec<String> {
    let mut found = vec![];
    for dir in CORE {
        let files = working_in(dir);
        assert!(!files.is_empty(), "{dir} has no sources");
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap();
            let shown = shown(&path);
            for (at, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                if let Some(effect) = effect(line) {
                    found.push(format!("{shown}:{}: {effect} in `{}`", at + 1, line.trim()));
                }
            }
        }
    }
    found
}

/// C1: the core opens no file, socket or process, reads no environment and prints nothing;
/// C2, R9: it sleeps nowhere, a wait is the caller's.
#[test]
fn the_core_names_no_effect() {
    let found = effects_in_the_core();
    assert!(
        found.is_empty(),
        "the core reaches the OS (C1, C2, R9): give it a port or a value from the app instead\n{}",
        found.join("\n")
    );
}

/// The scan sees what it is for, and nothing that only looks like it.
#[test]
fn the_scan_tells_an_effect_from_a_name_like_it() {
    for line in [
        "    std::thread::sleep(time);",
        "    if std::env::var_os(\"X\").is_some() {",
        "        eprintln!(\"{what}\");",
        "use std::{env, sync::Mutex};",
        "    let _ = std::io::stdout().flush();",
    ] {
        assert!(effect(line).is_some(), "not seen: {line}");
    }
    for line in [
        "    let fingerprint = fingerprint(&view);",
        "use std::{sync::Mutex, time::Duration};",
        "    self.environment.get(\"key\")",
    ] {
        assert_eq!(effect(line), None, "seen: {line}");
    }
}
