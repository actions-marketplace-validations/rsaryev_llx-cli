use std::fmt::Write as _;

use crate::testing::html;
use crate::{Dialect, Kind, parse, parse_as};

#[test]
fn a_footnote_is_its_number_where_it_is_named_and_where_it_is_told() {
    let doc = parse("A claim.[^a]\n\n[^a]: Its source.\n");
    assert_eq!(
        html(&doc),
        "<p>A claim.<sup>1</sup></p>\n<section class=\"footnotes\">\n<ol>\n<li value=\"1\">\n<p>Its source.</p>\n</li>\n</ol>\n</section>\n"
    );
    let mut blocks = doc.blocks();
    let reference = blocks.next().unwrap().children().nth(1).unwrap();
    assert_eq!(reference.kind(), Kind::FootnoteRef);
    assert_eq!(reference.start(), 1);
    let notes = blocks.next().unwrap();
    assert_eq!(notes.kind(), Kind::Footnotes);
    assert!(blocks.next().is_none());
    let note = notes.first().unwrap();
    assert_eq!((note.kind(), note.start()), (Kind::Footnote, 1));
}

#[test]
fn footnotes_are_numbered_in_the_order_they_are_first_referred_to() {
    let text = "[^b]: Bee.\n[^a]: Ay.\n\nFirst[^a], then[^b], again[^A].\n";
    let doc = parse(text);
    let html = html(&doc);
    assert!(
        html.starts_with("<p>First<sup>1</sup>, then<sup>2</sup>, again<sup>1</sup>.</p>\n"),
        "{html}"
    );
    assert!(
        html.ends_with("<li value=\"1\">\n<p>Ay.</p>\n</li>\n<li value=\"2\">\n<p>Bee.</p>\n</li>\n</ol>\n</section>\n"),
        "{html}"
    );
}

#[test]
fn a_footnote_never_referred_to_is_not_in_the_tree() {
    assert_eq!(html(&parse("Text.\n\n[^x]: Unused.\n")), "<p>Text.</p>\n");
    // The first definition of a label is the one told.
    let twice = html(&parse("A[^x]\n\n[^x]: One.\n\n[^x]: Two.\n"));
    assert!(twice.contains("One.") && !twice.contains("Two."), "{twice}");
}

#[test]
fn a_reference_with_no_definition_stays_text() {
    assert_eq!(html(&parse("A[^x] b")), "<p>A[^x] b</p>\n");
    assert_eq!(html(&parse("A[^x]\n\n[^y]: Y.")), "<p>A[^x]</p>\n");
}

#[test]
fn a_bang_before_a_footnote_is_an_exclamation_mark() {
    assert_eq!(
        html(&parse("Wow![^1]\n\n[^1]: N.")),
        "<p>Wow!<sup>1</sup></p>\n<section class=\"footnotes\">\n<ol>\n<li value=\"1\">\n<p>N.</p>\n</li>\n</ol>\n</section>\n"
    );
}

#[test]
fn a_definition_takes_its_indented_lines_and_leaves_the_flow_where_it_was() {
    let text =
        "Before[^n].\n\n[^n]: First line\n    goes on.\n\n    A second paragraph.\n\nAfter.\n";
    assert_eq!(
        html(&parse(text)),
        "<p>Before<sup>1</sup>.</p>\n<p>After.</p>\n<section class=\"footnotes\">\n<ol>\n<li value=\"1\">\n<p>First line\ngoes on.</p>\n<p>A second paragraph.</p>\n</li>\n</ol>\n</section>\n"
    );
}

#[test]
fn a_footnote_may_refer_to_another_numbered_after_the_text_s_own() {
    let text = "One[^a].\n\n[^a]: See[^c].\n[^c]: C.\n[^b]: B.\n\nTwo[^b].\n";
    let html = html(&parse(text));
    assert!(
        html.starts_with("<p>One<sup>1</sup>.</p>\n<p>Two<sup>2</sup>.</p>\n"),
        "{html}"
    );
    assert!(html.contains("<p>See<sup>3</sup>.</p>"), "{html}");
    let order: Vec<_> = ["See", "<p>B.", "<p>C."]
        .iter()
        .map(|it| html.find(it).unwrap())
        .collect();
    assert!(order.is_sorted(), "{html}");
}

#[test]
fn footnotes_are_read_with_githubs_extensions_and_not_in_commonmark() {
    let text = "A[^1]\n\n[^1]: N.\n";
    assert!(html(&parse_as(text, Dialect::Github)).contains("<sup>1</sup>"));
    let plain = html(&parse_as(text, Dialect::CommonMark));
    assert_eq!(plain, "<p>A<a href=\"N.\">^1</a></p>\n");
}

#[test]
fn many_references_and_definitions_are_read_in_time() {
    let mut notes = String::new();
    let mut refs = String::new();
    for n in 0..5_000 {
        writeln!(notes, "[^n{n}]: Note {n}.").unwrap();
        write!(refs, "x[^n{}] [^", n % 7).unwrap();
    }
    let cases = [
        refs + "\n\n" + &notes,
        "[^".repeat(30_000) + "\n\n[^a]: b",
        "![^a]".repeat(20_000) + "\n\n[^a]: b",
    ];
    let started = std::time::Instant::now();
    for case in &cases {
        assert!(parse(case).blocks().next().is_some());
    }
    assert!(started.elapsed().as_secs() < 10, "{:?}", started.elapsed());
}
