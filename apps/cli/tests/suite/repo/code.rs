//! The code rules (ARCHITECTURE.md, S4, S5, S6 and R8) and the unwinding W8 relies on,
//! checked on the manifests and sources.

use std::path::{Path, PathBuf};

use super::{crates, root, shown, walk, working};

/// Every crate's working code under `src/`, and its build script, also working code (S1).
fn working_sources() -> Vec<PathBuf> {
    let mut files = vec![];
    for dir in crates() {
        walk(&dir.join("src"), &working, &mut files);
        let build = dir.join("build.rs");
        if build.is_file() {
            files.push(build);
        }
    }
    files
}

/// S4: one set of lints for the workspace, taken by every crate.
#[test]
fn every_crate_takes_the_workspace_lints() {
    let found = crates();
    assert!(found.len() > 8, "found only {} crates", found.len());
    let missing: Vec<String> = found
        .iter()
        .filter(|dir| {
            let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
            !manifest.contains("[lints]\nworkspace = true")
        })
        .map(|dir| shown(dir))
        .collect();
    assert!(
        missing.is_empty(),
        "add `[lints] workspace = true` to {missing:?}"
    );
}

/// The crates that call no OS directly: the vocabulary, the core, the stores, the providers.
const SAFE: [&str; 7] = [
    "packages/client",
    "packages/contracts",
    "packages/protocol",
    "packages/agent",
    "packages/queue",
    "packages/session",
    "packages/providers",
];

/// S5: `unsafe` lives only where the OS is called, in the tools and the apps (tests aside).
#[test]
fn unsafe_lives_only_where_the_os_is_called() {
    let mut found = vec![];
    for dir in SAFE {
        let mut files = vec![];
        // Working code: a test may set the environment, which Rust marks unsafe.
        let rust = |path: &Path| {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.ends_with(".rs")
                && !name.ends_with("_tests.rs")
                && !path.starts_with(root().join(dir).join("tests"))
        };
        walk(&root().join(dir), &rust, &mut files);
        assert!(!files.is_empty(), "{dir} has no sources");
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap();
            let code = text.lines().filter(|line| !line.trim().starts_with("//"));
            if code.into_iter().any(|line| line.contains("unsafe ")) {
                found.push(shown(&path));
            }
        }
    }
    assert!(found.is_empty(), "unsafe outside tools and apps: {found:?}");
}

/// The lints that hold the rules (S4): removing one, or loosening it, fails here first.
const HELD: [&str; 13] = [
    "unwrap_used",
    "expect_used",
    "panic",
    "unreachable",
    "string_slice",
    "print_stdout",
    "print_stderr",
    "exit",
    "disallowed_types",
    "too_many_lines",
    "allow_attributes_without_reason",
    "allow_attributes",
    "unreachable_pub",
];

/// S4: the workspace's lints are hard: each is denied, and none of those that hold a rule is gone.
#[test]
fn the_workspace_lints_are_denied_and_kept() {
    let manifest = std::fs::read_to_string(root().join("Cargo.toml")).unwrap();
    let mut section = "";
    let mut levels = vec![];
    for line in manifest.lines() {
        if line.starts_with('[') {
            section = line;
        } else if section.starts_with("[workspace.lints")
            && let Some((name, level)) = line.split_once('=')
        {
            levels.push((name.trim().to_string(), level.trim().to_string()));
        }
    }
    let loose: Vec<_> = (levels.iter())
        .filter(|(_, level)| !matches!(level.as_str(), "\"deny\"" | "\"forbid\""))
        .collect();
    assert!(
        loose.is_empty(),
        "a workspace lint is not denied: {loose:?}"
    );
    let gone: Vec<_> = (HELD.iter())
        .filter(|held| !levels.iter().any(|(name, _)| name == *held))
        .collect();
    assert!(gone.is_empty(), "lints that hold a rule are gone: {gone:?}");
}

/// S4: working code is never exempt as a whole: an `allow` sits on the item it is about. Build
/// scripts, not walked here, fail the build by panicking, and say so once, at their top.
#[test]
fn no_working_file_turns_a_lint_off_whole() {
    let mut found = vec![];
    for dir in crates() {
        let mut files = vec![];
        walk(&dir.join("src"), &working, &mut files);
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap();
            let whole = |line: &str| line.starts_with("#![allow") || line.starts_with("#![expect");
            if text.lines().any(|line| whole(line.trim_start())) {
                found.push(shown(&path));
            }
        }
    }
    assert!(
        found.is_empty(),
        "a whole file is exempt from the lints: {found:?}"
    );
}

/// W8, R4: one request's panic ends only that request, which needs a panic that unwinds. The
/// release profile says so in so many words, and no profile aborts: a binary smaller by an abort
/// is a change of W8 first.
#[test]
fn every_build_unwinds_a_panic() {
    let manifest = std::fs::read_to_string(root().join("Cargo.toml")).unwrap();
    let mut section = "";
    let mut panics = vec![];
    for line in manifest.lines() {
        if line.starts_with('[') {
            section = line;
        } else if let Some((key, value)) = line.split_once('=')
            && key.trim() == "panic"
            && section.starts_with("[profile.")
        {
            panics.push((section, value.trim().trim_matches('"')));
        }
    }
    assert!(
        panics.contains(&("[profile.release]", "unwind")),
        "[profile.release] says `panic = \"unwind\"` (W8): {panics:?}"
    );
    let aborting: Vec<_> = panics.iter().filter(|(_, how)| *how != "unwind").collect();
    assert!(
        aborting.is_empty(),
        "a profile aborts on a panic (W8): {aborting:?}"
    );
}

/// The lints allowed in working code (S4's exceptions), at most, and at least: a ratchet. Each
/// `expect` is a rule set aside in one place for a reason; one more raises this number in the
/// commit that adds it, where a review sees it, and one fewer lowers it, so the room a fix made
/// is not taken again unseen.
const ALLOWED: usize = 32;

/// The lints `line` sets aside: an `allow` or `expect` attribute, or one inside a `cfg_attr`
/// (after its condition, or on a line of its own), but not a method or a function of that name.
fn set_aside(line: &str) -> usize {
    (["allow(", "expect("].iter())
        .map(|lint| {
            (line.match_indices(lint))
                .filter(|(at, _)| {
                    let before = line[..*at].trim_end();
                    before.is_empty() || before.ends_with(['[', ','])
                })
                .count()
        })
        .sum()
}

/// S4: the exceptions only fall. (Lines that are comments do not count; tests may allow freely.)
#[test]
fn the_lints_allowed_in_working_code_only_fall() {
    let mut found = 0;
    let mut by_file = vec![];
    for path in working_sources() {
        let text = std::fs::read_to_string(&path).unwrap();
        let here: usize = (text.lines().map(str::trim_start))
            .filter(|line| !line.starts_with("//"))
            .map(set_aside)
            .sum();
        if here > 0 {
            by_file.push(format!("  {here:>3}  {}", shown(&path)));
        }
        found += here;
    }
    assert!(
        found <= ALLOWED,
        "{found} lints allowed in working code, {ALLOWED} before: handle the case instead (S4), \
         or raise ALLOWED in code.rs in this commit, with its reason\n{}",
        by_file.join("\n")
    );
    assert!(
        found >= ALLOWED,
        "{found} lints allowed in working code, {ALLOWED} before: lower ALLOWED in code.rs to \
         {found}, so the ratchet keeps what was paid"
    );
}

/// S6: a file says what it is: every working file opens with its `//!`.
#[test]
fn every_working_file_says_what_it_is() {
    let files = working_sources();
    assert!(files.len() > 100, "found only {} files", files.len());
    let silent: Vec<String> = (files.iter())
        .filter(|path| {
            let text = std::fs::read_to_string(path).unwrap();
            !text.starts_with("//!")
        })
        .map(|path| shown(path))
        .collect();
    assert!(
        silent.is_empty(),
        "open each with a `//!` line on what it is: {silent:?}"
    );
}

/// R8: every thread is named: a `thread::Builder` gets its `.name(` before it spawns, so a
/// panic's message, a debugger and a profiler say whose thread it was.
#[test]
fn every_thread_builder_names_its_thread() {
    let builder = "thread::Builder::new()";
    let mut seen = 0;
    let mut unnamed = vec![];
    for path in working_sources() {
        let text = std::fs::read_to_string(&path).unwrap();
        for (at, _) in text.match_indices(builder) {
            seen += 1;
            let rest = &text[at + builder.len()..];
            let end = [rest.find(".spawn"), rest.find(';')]
                .into_iter()
                .flatten()
                .min()
                .unwrap_or(rest.len());
            if !rest[..end].contains(".name(") {
                let line = text[..at].matches('\n').count() + 1;
                unnamed.push(format!("{}:{line}", shown(&path)));
            }
        }
    }
    assert!(seen > 5, "found only {seen} thread builders");
    assert!(
        unnamed.is_empty(),
        "a thread spawned with no name: give its Builder a `.name(` (R8): {unnamed:?}"
    );
}
