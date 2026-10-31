//! The size budget: the whole project's working code stays under a hard limit. A small agent is a
//! goal, not an accident; when a change does not fit, code is removed, the limit is not raised.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use super::{crates, root, shown, walk, working};

/// Lines of working code in the whole workspace, at most. Hard: not to be raised. It was 17,000;
/// agents, parallel calls, steering, monitors and schedules took it to 20,000; the model catalog's
/// build script, background sessions and the terminal window to 25,000, then to 30,000 for the window's tabs
/// of every kind and the lanes' queue, then to 50,000 for the daemon, its thin client and what comes
/// after: the owner's raises.
const LIMIT: usize = 50_000;

/// Lines that are code: not blank and not a comment (a line may start with `*` as a
/// dereference, so only `//` lines are comments).
fn code_lines(text: &str) -> usize {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .count()
}

/// Each crate's working code under `src/` (tests and test-only helpers do not count) and its
/// build script, which is working code run at build time (its `tests/` directory is test code).
fn measure() -> (BTreeMap<String, usize>, Vec<(usize, PathBuf)>) {
    let mut packages = BTreeMap::new();
    let mut files = vec![];
    for dir in crates() {
        let mut found = vec![];
        walk(&dir.join("src"), &working, &mut found);
        let build = dir.join("build.rs");
        if build.is_file() {
            found.push(build);
        }
        for path in found {
            let lines = code_lines(&std::fs::read_to_string(&path).unwrap_or_default());
            *packages.entry(shown(&dir)).or_default() += lines;
            files.push((lines, path));
        }
    }
    files.sort_by_key(|file| std::cmp::Reverse(file.0));
    (packages, files)
}

#[test]
fn the_project_stays_under_its_size_limit() {
    let (packages, files) = measure();
    let total: usize = packages.values().sum();
    let mut report = format!("{total} lines of working code, the hard limit is {LIMIT}\n");
    for (name, lines) in &packages {
        let _ = writeln!(report, "  {lines:>6}  {name}");
    }
    report.push_str("the largest files:\n");
    for (lines, path) in files.iter().take(10) {
        let _ = writeln!(report, "  {lines:>6}  {}", shown(path));
    }
    println!("{report}");
    assert!(
        total <= LIMIT,
        "{report}remove code to make room; the limit is not raised"
    );
}

/// Lines of working code in one file, at most: a file is one idea, read whole. Hard, as the total
/// is: a file that grows past it is split along what it does, never let through.
const FILE_LIMIT: usize = 300;

#[test]
fn no_file_is_longer_than_its_limit() {
    let (_, files) = measure();
    let over: Vec<String> = files
        .iter()
        .filter(|(lines, _)| *lines > FILE_LIMIT)
        .map(|(lines, path)| format!("  {lines:>6}  {}", shown(path)))
        .collect();
    assert!(
        over.is_empty(),
        "files over {FILE_LIMIT} lines of working code; split each by what it does:\n{}",
        over.join("\n")
    );
}

#[test]
fn comments_blanks_and_tests_do_not_count() {
    let rust = "//! doc\n\nfn main() {\n    // why\n    *x = 1;\n}\n";
    assert_eq!(
        code_lines(rust),
        3,
        "the dereference counts, the comments do not"
    );
    for test in [
        "agent_tests.rs",
        "testing.rs",
        "test_support.rs",
        "fakes.rs",
    ] {
        assert!(!working(Path::new(test)), "{test}");
    }
    assert!(working(Path::new("agent.rs")));
}

#[test]
fn build_scripts_count() {
    let (_, files) = measure();
    let build = root().join("packages/providers/models/build.rs");
    assert!(
        files.iter().any(|(_, path)| *path == build),
        "{} is working code too",
        build.display()
    );
}
