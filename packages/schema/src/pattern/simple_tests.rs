use super::super::parse::parse;
use super::super::shape::branches;
use super::{Simple, simple};

fn form(source: &str) -> Option<Simple> {
    let Ok(node) = parse(source) else {
        panic!("{source} is no pattern")
    };
    let Some(shape) = branches(&node) else {
        panic!("{source} has no shape")
    };
    simple(&shape)
}

fn kind(source: &str) -> &'static str {
    match form(source) {
        None => "none",
        Some(Simple::Always) => "always",
        Some(Simple::Words(_)) => "words",
        Some(Simple::Run(_)) => "run",
    }
}

fn matches(source: &str, text: &str) -> bool {
    form(source).unwrap().is_match(text)
}

#[test]
fn each_form_is_found() {
    for (source, expected) in [
        (".*", "always"),
        ("^a*", "always"),
        ("x*$", "always"),
        ("a|b*", "always"),
        ("^a*$", "run"),
        ("^[a-z0-9_-]{3,16}$", "run"),
        ("^\\d$", "run"),
        ("^x-", "words"),
        ("^(small|medium|large)$", "words"),
        ("foo", "words"),
        ("@", "words"),
        ("^\\$", "words"),
        ("^$", "words"),
        ("^[a-z]+", "run"),
        ("[a-z]{2,3}$", "run"),
        ("\\d+", "run"),
        ("[a-zA-Z0-9_-]{162}", "run"),
        ("^ab+$", "none"),
        ("^a(b|c)$", "none"),
    ] {
        assert_eq!(kind(source), expected, "{source}");
    }
}

#[test]
fn words_match_where_their_anchors_say() {
    assert!(matches("^x-", "x-foo") && !matches("^x-", "ax-"));
    assert!(matches("end$", "the end") && !matches("end$", "endless"));
    assert!(matches("^(small|medium|large)$", "medium"));
    assert!(!matches("^(small|medium|large)$", "mediums"));
    assert!(matches("oo", "foo") && !matches("oo", "o"));
    assert!(matches("^dev|stable$", "devops") && matches("^dev|stable$", "unstable"));
    assert!(!matches("^dev|stable$", "a dev"));
    assert!(matches("^$", "") && !matches("^$", "a"));
}

#[test]
fn a_run_counts_characters_not_bytes() {
    assert!(matches("^[a-z0-9_-]{3,16}$", "user_01"));
    assert!(!matches("^[a-z0-9_-]{3,16}$", "ab"));
    assert!(!matches("^[a-z0-9_-]{3,16}$", "abcdefghijklmnopq"));
    assert!(!matches("^[a-z0-9_-]{3,16}$", "User"));
    assert!(matches("^.{2}$", "éé") && !matches("^.{2}$", "é"));
    assert!(matches("^[^a]+$", "😀b") && !matches("^.+$", "a\nb"));
    assert!(matches("^a*$", "") && !matches("^a+$", ""));
    assert!(matches("^\\s+$", "\u{3000}\u{FEFF}"));
}

#[test]
fn a_run_anchored_at_one_end_or_none_is_looked_for_there() {
    assert!(matches("^[a-z]{2,3}", "abcd1") && matches("^[a-z]{2,3}", "ab"));
    assert!(!matches("^[a-z]{2,3}", "a1bc") && !matches("^[a-z]{2,3}", ""));
    assert!(matches("[a-z]{2,3}$", "1abcd") && matches("[a-z]{2,3}$", "1ab"));
    assert!(!matches("[a-z]{2,3}$", "ab1") && !matches("[a-z]{2,3}$", "1a"));
    assert!(matches("\\d{3}", "ab123c") && matches("\\d{3}", "1\u{e9}234"));
    assert!(!matches("\\d{3}", "a12b34") && !matches("\\d{3}", "12\u{e9}3"));
    assert!(matches("[^a]{2}", "\u{e9}\u{1F600}") && !matches("[^a]{2}", "a\u{e9}a"));
    assert!(matches("\\d+", "a1") && !matches("\\d+", "abc"));
}
