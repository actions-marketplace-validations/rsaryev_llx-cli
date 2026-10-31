use std::path::Path;

use super::*;
use crate::testing::Scratch;

fn failed(result: Result<PathBuf, ToolError>) -> String {
    match result {
        Err(ToolError::Failed(message)) => message,
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn relative_paths_resolve_against_the_root_by_name() {
    let dir = Scratch::new();
    let workspace = Workspace::new(dir.path()).unwrap();
    let root = workspace.root().to_path_buf();
    assert_eq!(
        workspace.readable("src/./lib.rs").unwrap(),
        root.join("src/lib.rs")
    );
    assert_eq!(
        workspace.readable("src/../Cargo.toml").unwrap(),
        root.join("Cargo.toml")
    );
    assert_eq!(workspace.readable("  a.txt ").unwrap(), root.join("a.txt"));
}

#[test]
fn an_empty_path_is_refused() {
    let dir = Scratch::new();
    let workspace = Workspace::new(dir.path()).unwrap();
    assert_eq!(failed(workspace.readable("  ")), "the path is empty");
}

#[test]
fn absolute_paths_are_readable_anywhere_but_writable_only_inside() {
    let dir = Scratch::new();
    let workspace = Workspace::new(dir.path().join(".")).unwrap();
    let outside = std::env::temp_dir().join("elsewhere.txt");
    let shown = outside.to_str().unwrap();
    assert_eq!(workspace.readable(shown).unwrap(), outside);
    assert!(failed(workspace.writable(shown)).contains("outside the workspace"));
    assert!(failed(workspace.writable("../escape.txt")).contains("outside the workspace"));
    let inside = workspace.root().join("new/dir/file.rs");
    assert_eq!(workspace.writable("new/dir/file.rs").unwrap(), inside);
    assert_eq!(
        workspace.writable(inside.to_str().unwrap()).unwrap(),
        inside
    );
}

#[test]
fn a_symlink_out_of_the_root_is_not_writable() {
    let dir = Scratch::new();
    let elsewhere = Scratch::new();
    std::fs::create_dir_all(dir.path().join("root")).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), dir.path().join("root/out")).unwrap();
    let workspace = Workspace::new(dir.path().join("root")).unwrap();
    assert!(failed(workspace.writable("out/file.txt")).contains("outside the workspace"));
}

#[test]
fn a_write_through_a_symlink_inside_lands_on_its_target() {
    let dir = Scratch::new();
    let target = dir.file("real.txt", "x");
    std::os::unix::fs::symlink(&target, dir.path().join("link.txt")).unwrap();
    let workspace = Workspace::new(dir.path()).unwrap();
    assert_eq!(
        workspace.writable("link.txt").unwrap(),
        workspace.root().join("real.txt")
    );
}

#[test]
fn reports_show_paths_relative_to_the_root() {
    let dir = Scratch::new();
    let workspace = Workspace::new(dir.path()).unwrap();
    let root = workspace.root().to_path_buf();
    assert_eq!(workspace.shown(&root), ".");
    assert_eq!(
        workspace.shown(&root.join("src").join("lib.rs")),
        Path::new("src").join("lib.rs").display().to_string()
    );
    let outside = std::env::temp_dir().join("x.txt");
    assert_eq!(workspace.shown(&outside), outside.display().to_string());
}

#[test]
fn a_denied_path_is_refused_however_it_is_reached() {
    let dir = Scratch::new();
    let secret = Scratch::new();
    let key = secret.file("llx/credentials", "sk-or-secret");
    let sessions = secret.path().join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let workspace = Workspace::new(dir.path())
        .unwrap()
        .denying([key.clone(), sessions.clone()]);
    for path in [&key, &sessions.join("1-ab.json")] {
        let message = failed(workspace.readable(path.to_str().unwrap()));
        assert!(message.ends_with("is off limits to the tools"), "{message}");
        assert!(failed(workspace.writable(path.to_str().unwrap())).contains("off limits"));
    }
    {
        std::os::unix::fs::symlink(&key, dir.path().join("innocent.txt")).unwrap();
        assert!(failed(workspace.readable("innocent.txt")).contains("off limits"));
    }
    assert!(workspace.readable("anything-else.txt").is_ok());
}
