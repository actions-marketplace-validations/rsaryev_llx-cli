use super::super::{Hunk, parse};
use super::*;

/// The chunks of a one-file update patch made of `body`.
fn chunks(body: &str) -> Vec<Chunk> {
    let text = format!("*** Begin Patch\n*** Update File: f.txt\n{body}*** End Patch");
    let Hunk::Update { chunks, .. } = parse(&text).unwrap().remove(0) else {
        panic!("an update");
    };
    chunks
}

fn applied(content: &str, body: &str) -> Result<String, String> {
    update(content, &chunks(body), "f.txt").map(|(text, _)| text)
}

/// The diff of `body` applied to `content`.
fn diffed(content: &str, body: &str) -> String {
    update(content, &chunks(body), "f.txt").unwrap().1
}

#[test]
fn the_diff_shows_each_edit_as_the_patch_made_it() {
    assert_eq!(diffed("", "@@\n+a\n"), "@@ -0,0 +1,1 @@\n+a\n");
    assert_eq!(diffed("a\n", "@@\n-a\n"), "@@ -1,1 +0,0 @@\n-a\n");
    assert_eq!(diffed("a\n", "@@\n a\n"), "");
    // Line endings as the file has them, the added lines' too.
    assert_eq!(
        diffed("a\r\nb\r\n", "@@\n-a\n+A\n"),
        "@@ -1,2 +1,2 @@\n-a\r\n+A\r\n b\r\n"
    );
    // A last line without a newline is given one.
    assert_eq!(
        diffed("a\nb", "@@\n-a\n+A\n"),
        "@@ -1,2 +1,2 @@\n-a\n+A\n-b\n\\ No newline at end of file\n+b\n"
    );
    assert_eq!(
        diffed("a\nb", "@@\n-b\n+B\n"),
        "@@ -1,2 +1,2 @@\n a\n-b\n\\ No newline at end of file\n+B\n"
    );
    // Context lines matched loosely are shown as the file has them, not as the patch spells them.
    assert_eq!(
        diffed("  keep   \nold\n", "@@\n keep\n-old\n+new\n"),
        "@@ -1,2 +1,2 @@\n   keep   \n-old\n+new\n"
    );
}

#[test]
fn lines_are_replaced_where_they_are() {
    assert_eq!(applied("a\nb\nc\n", "@@\n-b\n+B\n").unwrap(), "a\nB\nc\n");
    assert_eq!(
        applied("a\nb\nc\nd\n", "@@\n-a\n+A\n@@\n c\n-d\n+D\n+E\n").unwrap(),
        "A\nb\nc\nD\nE\n"
    );
}

#[test]
fn a_context_line_picks_the_place_among_equal_ones() {
    let file = "fn one() {\n    x\n}\nfn two() {\n    x\n}\n";
    assert_eq!(
        applied(file, "@@ fn two() {\n-    x\n+    y\n").unwrap(),
        "fn one() {\n    x\n}\nfn two() {\n    y\n}\n"
    );
    let why = applied(file, "@@ fn three() {\n-    x\n+    y\n").unwrap_err();
    assert!(
        why.contains("fn three() {") && why.contains("f.txt"),
        "{why}"
    );
}

#[test]
fn lines_only_added_go_to_the_end() {
    assert_eq!(applied("a\n", "@@\n+b\n").unwrap(), "a\nb\n");
    assert_eq!(applied("", "@@\n+b\n").unwrap(), "b\n");
}

#[test]
fn the_end_of_file_marker_picks_the_last_place() {
    assert_eq!(
        applied("x\ny\nx\n", "@@\n-x\n+z\n*** End of File\n").unwrap(),
        "x\ny\nz\n"
    );
}

#[test]
fn context_lines_stay_as_the_file_has_them() {
    // Matched ignoring whitespace, but not rewritten as the patch spells them.
    assert_eq!(
        applied("  keep   \nold\n", "@@\n keep\n-old\n+new\n").unwrap(),
        "  keep   \nnew\n"
    );
}

#[test]
fn line_endings_and_the_last_newline_are_kept_or_made() {
    assert_eq!(
        applied("a\r\nb\r\nc\r\n", "@@\n-b\n+B\n+B2\n").unwrap(),
        "a\r\nB\r\nB2\r\nc\r\n"
    );
    assert_eq!(applied("a\nb", "@@\n-b\n+B\n").unwrap(), "a\nB\n");
}

#[test]
fn a_blank_context_line_past_the_last_line_is_forgiven() {
    assert_eq!(applied("a\nb\n", "@@\n a\n-b\n+B\n\n").unwrap(), "a\nB\n");
}

#[test]
fn lines_not_found_are_named_with_the_file() {
    let why = applied("a\nb\n", "@@\n-b\n-c\n+x\n").unwrap_err();
    assert!(why.contains("f.txt") && why.contains("b\nc"), "{why}");
    // Each chunk is looked for after the one before it.
    let why = applied("a\nb\n", "@@\n-b\n+B\n@@\n-a\n+A\n").unwrap_err();
    assert!(why.contains("f.txt") && why.contains('a'), "{why}");
}

#[test]
fn lines_not_found_in_a_short_file_come_back_with_the_whole_file_numbered() {
    // The model wrote lines as it remembered them, or lines the file never had: a short file is
    // given back whole, numbered, so the next patch needs no read first.
    let file = "fn a() {}\nfn run(x: u32) {\n    go(x);\n}\nfn b() {}\n";
    let whole = "f.txt as it is now:\n     1\tfn a() {}\n     2\tfn run(x: u32) {\n     3\t    \
                 go(x);\n     4\t}\n     5\tfn b() {}";
    for body in [
        "@@\n fn run(x: i32) {\n-    go(x);\n+    go(x + 1);\n",
        "@@\n-nothing like it\n+x\n",
        "@@ fn gone() {\n-    go(x);\n+    y\n",
    ] {
        let why = applied(file, body).unwrap_err();
        assert!(why.contains(whole), "{body}: {why}");
    }
}

#[test]
fn lines_not_found_in_a_long_file_come_back_with_its_nearest_lines() {
    let mut file = (1..=2_000).fold(String::new(), |mut file, n| {
        let _ = writeln!(file, "let v{n} = {n};");
        file
    });
    file.push_str("fn run(x: u32) {\n    go(x);\n}\n");
    let why = applied(
        &file,
        "@@\n fn run(x: i32) {\n-    go(x);\n+    go(x + 1);\n",
    )
    .unwrap_err();
    assert!(
        why.contains("nearest lines in f.txt, from line 2001:\nfn run(x: u32) {\n    go(x);"),
        "{why}"
    );
    assert!(!why.contains("let v1 = 1;"), "{why}");
    // Lines like none of the file's: nothing near to show.
    let why = applied(&file, "@@\n-nothing like it\n+x\n").unwrap_err();
    assert!(!why.contains("nearest"), "{why}");
}

#[test]
fn the_lines_added_and_removed_are_counted_without_the_context() {
    let all = chunks("@@\n keep\n-a\n-b\n+c\n@@\n+d\n");
    assert_eq!(counts(&all), (2, 2));
}

#[test]
fn lines_only_added_after_an_anchor_go_below_it() {
    let file = "fn one() {\n}\nfn two() {\n}\n";
    assert_eq!(
        applied(file, "@@ fn one() {\n+    x\n").unwrap(),
        "fn one() {\n    x\n}\nfn two() {\n}\n"
    );
}

#[test]
fn a_hunk_may_start_with_its_anchor_line() {
    let file = "fn one() {\n}\nfn two() {\n    x\n}\n";
    assert_eq!(
        applied(file, "@@ fn two() {\n-fn two() {\n+fn three() {\n").unwrap(),
        "fn one() {\n}\nfn three() {\n    x\n}\n"
    );
}
