use std::sync::mpsc;
use std::time::Duration;

use super::*;

fn files() -> Files {
    Files::with(
        [
            "README.md",
            "src/main.rs",
            "src/tui/markdown.rs",
            "src/tui/mod.rs",
            "site/index.html",
        ]
        .map(str::to_string)
        .to_vec(),
    )
}

#[test]
fn a_file_is_found_by_its_name_then_its_path_then_its_letters() {
    assert_eq!(files().matching("mod"), ["src/tui/mod.rs"]);
    assert_eq!(
        files().matching("ma"),
        ["src/main.rs", "src/tui/markdown.rs"]
    );
    assert_eq!(
        files().matching("tui"),
        ["src/tui/mod.rs", "src/tui/markdown.rs"]
    );
    assert_eq!(files().matching("sih"), ["site/index.html"]);
    assert!(files().matching("zzz").is_empty());
}

#[test]
fn nothing_is_offered_before_the_list_is_read() {
    assert!(Files::default().matching("").is_empty());
}

#[test]
fn a_path_is_found_whatever_its_case() {
    let files = Files::with(vec!["docs/ReadMe.MD".into()]);
    assert_eq!(files.matching("readme"), ["docs/ReadMe.MD"]);
    assert_eq!(files.matching("README.md"), ["docs/ReadMe.MD"]);
}

/// `@` must not hold up the interface while a large tree is listed: the list is read on a thread
/// of its own and comes back as an input, once.
#[test]
fn the_list_is_read_off_the_interface_s_thread_and_arrives_once() {
    let dir = super::super::test_support::Scratch::new();
    for (path, text) in [
        ("src/Main.rs", ""),
        ("target/out.rs", ""),
        (".hidden/x.rs", ""),
    ] {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let (arrive, arrived) = mpsc::sync_channel(2);
    let mut files = Files::new(
        dir.path().to_path_buf(),
        Box::new(move |list| {
            let _ = arrive.send(list);
        }),
    );
    files.load();
    assert!(files.matching("main").is_empty(), "not read on this thread");
    let list = arrived.recv_timeout(Duration::from_secs(10)).unwrap();
    files.loaded(list);
    assert_eq!(files.matching("main"), ["src/Main.rs"]);
    assert!(files.matching("out").is_empty(), "build output is left out");
    files.load();
    assert!(arrived.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn of_many_matches_the_closest_are_offered_in_order() {
    let mut list: Vec<String> = (0..30)
        .map(|n| format!("deep/dir/notes_{n:02}.txt"))
        .collect();
    list.extend([
        "src/notebook.rs".into(),
        "a/b/c/x/note.rs".into(),
        "note.md".into(),
    ]);
    // Each name begins with it: the shorter path first, then the first in order.
    let mut closest = ["note.md", "a/b/c/x/note.rs", "src/notebook.rs"]
        .map(String::from)
        .to_vec();
    closest.extend((0..5).map(|n| format!("deep/dir/notes_{n:02}.txt")));
    assert_eq!(Files::with(list).matching("note"), closest);
}

#[test]
fn outside_git_a_linked_folder_is_one_entry_not_walked_into() {
    let dir = crate::test_support::Scratch::new();
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("sub/b.txt"), "b").unwrap();
    std::os::unix::fs::symlink(".", dir.path().join("loop")).unwrap();
    let mut listed = list(dir.path());
    listed.sort_unstable();
    assert_eq!(listed, ["loop", "sub/b.txt"]);
}
