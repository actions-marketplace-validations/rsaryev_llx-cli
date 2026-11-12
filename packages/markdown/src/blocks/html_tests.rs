use crate::parse;
use crate::testing::html;

/// Each text, and how many blocks it reads into: where its HTML block closes says which.
#[test]
fn an_html_block_closes_on_the_line_that_holds_its_close() {
    let long = "x".repeat(300);
    let cases = [
        ("<!-- a -->\nafter\n", 2),
        ("<!-- a\nb -->\nafter\n", 2),
        ("<!-- a > b ->\n-- >\nc -->\nafter\n", 2),
        ("<!-- a\nb\nafter\n", 1),
        ("   <!-- a -->\nafter\n", 2),
        ("<?php a > b\n?>\nafter\n", 2),
        ("<!DOCTYPE html>\nafter\n", 2),
        ("<![CDATA[ a ]>\n]]>\nafter\n", 2),
        ("<pre>\na > b\n</pre>\nafter\n", 2),
        ("<script>\nno close\nafter\n", 1),
        (&format!("<!-- {long}\n{long} -->\nafter\n"), 2),
        (
            &format!("<!-- {long} > {long}\n{long}-->{long}\nafter\n"),
            2,
        ),
    ];
    for (text, blocks) in cases {
        let doc = parse(text);
        assert_eq!(doc.root().children().count(), blocks, "{text:?}");
        let quoted = format!("> {}", text.trim_end().replace('\n', "\n> "));
        let doc = parse(&quoted);
        let quote = doc.root().first().unwrap();
        assert_eq!(quote.children().count(), blocks, "{quoted:?}");
    }
}

#[test]
fn what_follows_a_closed_comment_is_markdown_again() {
    let doc = parse("<!-- a\n*b* -->\n*c*\n");
    assert_eq!(html(&doc), "<!-- a\n*b* -->\n<p><em>c</em></p>\n");
}
