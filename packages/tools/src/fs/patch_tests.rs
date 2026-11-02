use serde_json::json;

use super::*;

fn patch(body: &str) -> String {
    format!("*** Begin Patch\n{body}*** End Patch")
}

fn hunks(body: &str) -> Vec<Hunk> {
    parse(&patch(body)).unwrap()
}

fn lines(all: &[&str]) -> Vec<String> {
    all.iter().map(ToString::to_string).collect()
}

#[test]
fn each_kind_of_section_is_read() {
    let read = hunks(
        "*** Add File: new.txt\n+one\n+two\n\
         *** Delete File: old.txt\n\
         *** Update File: src/a.rs\n*** Move to: src/b.rs\n\
         @@ fn f()\n-    pass\n+    return 1\n\
         @@\n keep\n+added\n",
    );
    assert_eq!(
        read,
        vec![
            Hunk::Add {
                path: "new.txt".into(),
                content: "one\ntwo\n".into()
            },
            Hunk::Delete {
                path: "old.txt".into()
            },
            Hunk::Update {
                path: "src/a.rs".into(),
                to: Some("src/b.rs".into()),
                chunks: vec![
                    Chunk {
                        context: Some("fn f()".into()),
                        old: lines(&["    pass"]),
                        new: lines(&["    return 1"]),
                        ..Chunk::default()
                    },
                    Chunk {
                        context: None,
                        old: lines(&["keep"]),
                        new: lines(&["keep", "added"]),
                        kept: vec![(0, 0)],
                        eof: false,
                    },
                ],
            },
        ]
    );
}

#[test]
fn the_first_hunk_needs_no_at_line_and_a_blank_line_is_context() {
    let read = hunks("*** Update File: f\n keep\n\n-old\n+new\n");
    assert_eq!(
        read,
        vec![Hunk::Update {
            path: "f".into(),
            to: None,
            chunks: vec![Chunk {
                context: None,
                old: lines(&["keep", "", "old"]),
                new: lines(&["keep", "", "new"]),
                kept: vec![(0, 0), (1, 1)],
                eof: false,
            }],
        }]
    );
}

#[test]
fn end_of_file_marks_the_hunk_and_only_an_at_line_may_follow() {
    let read = hunks("*** Update File: f\n@@\n-a\n+b\n*** End of File\n\n@@\n-c\n+d\n");
    let Hunk::Update { chunks, .. } = &read[0] else {
        panic!("{read:?}");
    };
    assert!(chunks[0].eof && !chunks[1].eof, "{chunks:?}");
    let why = parse(&patch(
        "*** Update File: f\n@@\n-a\n+b\n*** End of File\n-c\n",
    ))
    .unwrap_err();
    assert!(why.contains("line 7") && why.contains("@@"), "{why}");
}

#[test]
fn a_move_alone_is_a_section() {
    let read = hunks("*** Update File: a\n*** Move to: b\n");
    assert_eq!(
        read,
        vec![Hunk::Update {
            path: "a".into(),
            to: Some("b".into()),
            chunks: vec![]
        }]
    );
}

#[test]
fn markers_may_have_spaces_around_them_and_lines_end_in_crlf() {
    let text = " *** Begin Patch \r\n*** Add File: a\r\n+x\r\n  *** End Patch  \n";
    assert_eq!(
        parse(text).unwrap(),
        vec![Hunk::Add {
            path: "a".into(),
            content: "x\n".into()
        }]
    );
}

#[test]
fn a_heredoc_around_the_patch_is_taken_off() {
    for open in ["<<EOF", "<<'EOF'", "<<\"EOF\""] {
        let text = format!("{open}\n{}\nEOF\n", patch("*** Delete File: a\n"));
        assert_eq!(
            parse(&text).unwrap(),
            vec![Hunk::Delete { path: "a".into() }],
            "{open}"
        );
    }
}

#[test]
fn a_wrong_line_is_numbered_as_the_text_has_it_heredoc_and_blank_lines_too() {
    // `stray` is the text's fifth line each time: the model finds it by the number it is told.
    for text in [
        "*** Begin Patch\n*** Add File: a\n+x\n+y\nstray\n*** End Patch",
        "<<'EOF'\n*** Begin Patch\n*** Add File: a\n+x\nstray\n*** End Patch\nEOF",
        "\n\n*** Begin Patch\n*** Add File: a\nstray\n*** End Patch",
    ] {
        let why = parse(text).unwrap_err();
        assert!(
            why.starts_with("invalid patch, line 5: "),
            "{text:?}: {why}"
        );
    }
}

#[test]
fn a_patch_that_is_not_one_says_what_is_wrong_and_where() {
    let wrong = |text: &str| parse(text).unwrap_err();
    assert!(wrong("hello").contains("*** Begin Patch"));
    assert!(wrong("*** Begin Patch\n*** Delete File: a\n").contains("*** End Patch"));
    assert!(wrong(&patch("")).contains("no file"));
    // A line that starts none of the sections.
    let why = wrong(&patch("*** Add File: a\n+x\nstray\n"));
    assert!(why.contains("line 4") && why.contains("stray"), "{why}");
    let why = wrong(&patch("*** Update File: a\n@@\n-x\nstray\n"));
    assert!(why.contains("line 5") && why.contains("stray"), "{why}");
    // Nothing to change.
    let why = wrong(&patch("*** Update File: a.rs\n*** Delete File: b\n"));
    assert!(why.contains("a.rs"), "{why}");
    let why = wrong(&patch("*** Update File: a.rs\n@@\n"));
    assert!(why.contains("a.rs"), "{why}");
    // Text after the end.
    assert!(wrong("*** Begin Patch\n*** End Patch\nmore").contains("*** End Patch"));
}

#[test]
fn the_paths_of_a_call_are_every_file_of_its_patch() {
    let patch = Value::String(patch(
        "*** Update File: a\n*** Move to: b\n*** Add File: c\n+x\n*** Update File: a\n@@\n-x\n+y\n",
    ));
    assert_eq!(paths(&patch), ["a", "b", "c"]);
    // Written as JSON, it is refused: it names no file.
    assert!(paths(&json!({"patch": patch.as_str()})).is_empty());
    assert!(paths(&Value::String("not a patch".into())).is_empty());
    assert!(paths(&json!({})).is_empty());
}

#[test]
fn an_empty_hunk_left_after_the_others_is_let_go() {
    // A model closes its last hunk with a `@@` it never fills.
    let hunks = parse(&patch(
        "*** Update File: a\n@@\n-x\n+y\n@@\n*** Update File: b\n@@\n-p\n+q\n@@\n",
    ))
    .unwrap();
    assert_eq!(hunks.len(), 2);
    for hunk in &hunks {
        let Hunk::Update { chunks, .. } = hunk else {
            panic!("two updates: {hunks:?}");
        };
        assert_eq!(chunks.len(), 1, "{hunks:?}");
    }
    // Two `@@` lines with nothing between: the empty one is let go too.
    assert!(parse(&patch("*** Update File: a\n@@\n@@\n-x\n+y\n")).is_ok());
}
