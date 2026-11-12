//! The examples of the CommonMark specification, and of GitHub's extensions to it: each read and
//! printed as HTML is what the specification gives.

use crate::testing::html;
use crate::{Dialect, parse_as};

fn unescaped(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next().unwrap() {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'u' => {
                let hex: String = chars.by_ref().skip(1).take_while(|ch| *ch != '}').collect();
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).unwrap()).unwrap());
            }
            other => out.push(other),
        }
    }
    out
}

/// The examples that read otherwise than expected, each with what it gave.
fn failures(examples: &str, dialect: Dialect) -> (usize, Vec<String>) {
    let lines: Vec<&str> = examples.lines().collect();
    let mut failed = Vec::new();
    for example in lines.chunks(3) {
        let (markdown, expected) = (unescaped(example[1]), unescaped(example[2]));
        let got = html(&parse_as(&markdown, dialect));
        if got != expected {
            failed.push(format!(
                "{}\n  markdown {markdown:?}\n  expected {expected:?}\n  got      {got:?}",
                example[0]
            ));
        }
    }
    (lines.len() / 3, failed)
}

#[test]
fn every_example_of_commonmark_reads_as_the_specification_says() {
    let (count, failed) = failures(include_str!("../data/commonmark.txt"), Dialect::CommonMark);
    assert_eq!(count, 652);
    assert!(
        failed.is_empty(),
        "{} failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

#[test]
fn commonmark_reads_the_same_with_githubs_extensions_on() {
    // But where an extension is what the example is about: a bare address, a tilde.
    let (_, failed) = failures(include_str!("../data/commonmark.txt"), Dialect::Github);
    let known = ["# 608 Autolinks", "# 611 Autolinks"];
    let other: Vec<_> = failed
        .iter()
        .filter(|failure| !known.iter().any(|known| failure.starts_with(known)))
        .collect();
    assert!(other.is_empty(), "{} failed:\n{other:#?}", other.len());
}

#[test]
fn every_example_of_githubs_extensions_reads_as_github_says() {
    let (count, failed) = failures(include_str!("../data/github.txt"), Dialect::Github);
    assert_eq!(count, 23);
    // A bare e-mail address is left as text: nothing an agent's reader does with one.
    let known = ["# 613 ", "# 614 ", "# 615 "];
    let other: Vec<_> = failed
        .iter()
        .filter(|failure| !known.iter().any(|known| failure.starts_with(known)))
        .collect();
    assert_eq!(failed.len() - other.len(), known.len());
    assert!(other.is_empty(), "{} failed:\n{other:#?}", other.len());
}

#[test]
fn a_text_cut_before_its_last_line_ending_reads_the_same() {
    // What a reply is while it still arrives: its last line has no ending yet.
    let examples = include_str!("../data/commonmark.txt");
    let lines: Vec<&str> = examples.lines().collect();
    let mut differ = Vec::new();
    for example in lines.chunks(3) {
        let whole = unescaped(example[1]);
        let Some(cut) = whole.strip_suffix('\n') else {
            continue;
        };
        // A line of blanks alone is not the same text without its ending.
        if cut.ends_with([' ', '\t', '\n']) {
            continue;
        }
        let (whole, cut) = (
            parse_as(&whole, Dialect::Github),
            parse_as(cut, Dialect::Github),
        );
        if html(&whole) != html(&cut) {
            differ.push(example[0]);
        }
    }
    assert!(differ.is_empty(), "{differ:#?}");
}

#[test]
fn a_last_line_with_no_ending_stays_a_cut_of_the_source() {
    for text in ["one\ntwo", "<div>\none\ntwo", "- one\n  two"] {
        let doc = parse_as(text, Dialect::Github);
        let mut node = doc.blocks().next().unwrap();
        while let Some(inner) = node.first() {
            node = inner;
        }
        let (source, read) = (text.as_bytes().as_ptr_range(), node.text().as_ptr());
        assert!(source.contains(&read), "{text:?}: {:?}", node.text());
    }
}
