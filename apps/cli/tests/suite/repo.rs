//! The repository itself, as ARCHITECTURE.md binds it: its layers (`architecture`), the core that
//! names no effect (`core`), the code rules (`code`) and the size budget (`budget`).

use std::path::{Path, PathBuf};

mod architecture;
mod budget;
mod code;
mod core;

/// The workspace's root.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every file under `dir` that `keep` takes, its subdirectories walked.
fn walk(dir: &Path, keep: &dyn Fn(&Path) -> bool, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, keep, out);
        } else if keep(&path) {
            out.push(path);
        }
    }
}

/// Every crate's directory: the packages, the providers a level down, the apps.
fn crates() -> Vec<PathBuf> {
    let mut out = vec![];
    for group in ["packages", "packages/providers", "apps"] {
        for entry in std::fs::read_dir(root().join(group)).unwrap().flatten() {
            if entry.path().join("Cargo.toml").is_file() {
                out.push(entry.path());
            }
        }
    }
    out
}

/// Working code: a Rust source that is not a test beside the code (`*_tests.rs`) or a test-only
/// helper (`testing.rs`, `test_support.rs`, `fakes.rs`).
fn working(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.ends_with(".rs")
        && !name.ends_with("_tests.rs")
        && !matches!(name, "testing.rs" | "test_support.rs" | "fakes.rs")
}

/// `path` as the repository names it, from its root.
fn shown(path: &Path) -> String {
    path.strip_prefix(root())
        .unwrap_or(path)
        .display()
        .to_string()
}
