use ratatui::text::{Line, Span};

use super::*;
use crate::style;

fn rows(text: &str, width: usize) -> Vec<String> {
    let line = Line::from(text.to_string());
    let first = Line::from("• ");
    let rest = Line::from("  ");
    wrap(&line, width, &first, &rest)
        .iter()
        .map(style::plain)
        .collect()
}

#[test]
fn words_wrap_under_their_own_start_and_never_break_mid_word() {
    insta::assert_snapshot!(rows("ls -la && find . -maxdepth 2 -not -path '*/node_modules*' -not -path '*/.git/*' | head -100", 40).join("\n"), @r"
    • ls -la && find . -maxdepth 2 -not
      -path '*/node_modules*' -not -path
      '*/.git/*' | head -100
    ");
}

#[test]
fn a_word_wider_than_a_row_is_split_and_nothing_is_wider_than_the_width() {
    let lines = rows(&"x".repeat(25), 10);
    assert_eq!(lines, ["• xxxxxxxx", "  xxxxxxxx", "  xxxxxxxx", "  x"]);
    assert!(lines.iter().all(|line| width(line) <= 10));
}

#[test]
fn width_counts_columns_not_bytes() {
    assert_eq!(width("caf\u{e9}"), 4);
    assert_eq!(width("\u{65e5}\u{672c}"), 4);
    assert_eq!(
        rows("na\u{ef}ve r\u{e9}sum\u{e9} for a caf\u{e9}", 16),
        ["• na\u{ef}ve r\u{e9}sum\u{e9}", "  for a caf\u{e9}"]
    );
}

#[test]
fn styles_stay_with_their_words() {
    let line = Line::from(vec![
        Span::raw("run "),
        Span::styled("cargo test", style::bold()),
    ]);
    let wrapped = wrap(&line, 9, &Line::from(""), &Line::from(""));
    assert_eq!(
        wrapped.iter().map(style::plain).collect::<Vec<_>>(),
        ["run cargo", "test"]
    );
    assert_eq!(wrapped[1].spans[0].style, style::bold());
}

#[test]
fn cut_ends_in_an_ellipsis() {
    assert_eq!(cut("abcdef", 4), "abc…");
    assert_eq!(cut("abc", 4), "abc");
}

#[test]
fn code_breaks_anywhere_and_keeps_its_indentation() {
    assert_eq!(hard("    let x = 1;", 8), ["    let ", "x = 1;"]);
    assert_eq!(hard("", 8), [""]);
}
