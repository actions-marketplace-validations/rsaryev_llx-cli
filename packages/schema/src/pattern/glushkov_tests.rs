use std::slice;

use super::super::bits::Bits;
use super::super::parse::parse;
use super::{Automaton, positions, shortest};

/// The answer of the automaton, and of its bit-parallel walk where it is a DFA: the same.
fn matches<B: Bits>(source: &str, text: &str) -> bool {
    let automaton = Automaton::<B>::new(&parse(source).unwrap());
    let answer = automaton.is_match(text);
    if !automaton.always {
        assert_eq!(
            automaton.parallel(text),
            answer,
            "{source} on {text:?}, bit-parallel"
        );
    }
    answer
}

/// The same answer at every width.
fn check(source: &str, text: &str, expected: bool) {
    assert_eq!(
        matches::<u64>(source, text),
        expected,
        "{source} on {text:?}"
    );
    assert_eq!(
        matches::<u128>(source, text),
        expected,
        "{source} on {text:?} (u128)"
    );
    assert_eq!(
        matches::<[u64; 4]>(source, text),
        expected,
        "{source} on {text:?} (256)"
    );
    assert_eq!(
        matches::<[u64; 8]>(source, text),
        expected,
        "{source} on {text:?} (512)"
    );
}

#[test]
fn counted_repetition_is_written_out() {
    let count = |source: &str| {
        let node = parse(source).unwrap();
        positions(slice::from_ref(&node))
    };
    assert_eq!(count("^\\d{4}-\\d{2}$"), 9, "an anchor is a marker");
    assert_eq!(count("a{2,5}b+c*"), 7);
    assert_eq!(count("(ab){3}|c"), 7);
    assert_eq!(count("a{0}"), 0);
    assert_eq!(count("(a{1000}){1000}"), 1_000_000);
    assert_eq!(count("\\ba\\B"), 3, "a word boundary is a marker too");
}

#[test]
fn a_text_shorter_than_any_match_is_refused() {
    assert_eq!(shortest(&parse("a{3}b?(cd|e)").unwrap()), 4);
    assert_eq!(shortest(&parse("^$|x").unwrap()), 0);
    check("[a-z]{5}", "abcd", false);
    check("[a-z]{5}", "abcde", true);
    check("^.{3}$", "日本語", true);
}

#[test]
fn anchors_anywhere() {
    check("a^b", "a^b", false);
    check("a$b", "ab", false);
    check("^([^,]|$)", "", true);
    check("^([^,]|$)", "a", true);
    check("^([^,]|$)", ",a", false);
    check("(^a|b)c", "ac", true);
    check("(^a|b)c", "xac", false);
    check("(^a|b)c", "xbc", true);
    check("x(a$|b)", "xa", true);
    check("x(a$|b)", "xab", false);
    check("x(a$|b)", "zxbz", true);
    check("^$[a-z]*$", "", true);
    check("^$[a-z]*$", "a", false);
    check("$^", "", true);
    check("$^", "a", false);
    check("(^|,)x", ",x", true);
    check("(^|,)x", "ax", false);
    check("(a|^)b", "b", true);
    check("(a|^)b", "xb", false);
    check("(a|^)b", "xab", true);
    check("(^|a)", "zzz", true);
    check("(a|$)", "zzz", true);
    check("^(\\/?(\\.{2}|[a-z0-9\\-]*)($|\\/))*$", "/a/b", true);
    check("^(\\/?(\\.{2}|[a-z0-9\\-]*)($|\\/))*$", "A", false);
    check("(^a)+b", "ab", true);
    check("(^a)+b", "aab", false);
}

#[test]
fn anchored_patterns() {
    check("^\\d{4}-\\d{2}-\\d{2}$", "2024-01-31", true);
    check("^\\d{4}-\\d{2}-\\d{2}$", "2024-01-31x", false);
    check("^\\d{4}-\\d{2}-\\d{2}$", "x2024-01-31", false);
    check("^\\d{4}-\\d{2}-\\d{2}$", "2024-1-31", false);
    check("^(\\+\\d{1,3})?\\d{10}$", "+441234567890", true);
    check("^(\\+\\d{1,3})?\\d{10}$", "1234567890", true);
    check("^(\\+\\d{1,3})?\\d{10}$", "+12345", false);
    check("^(\\+\\d{1,3})?\\d{10}$", "+12345678901234", false);
    check(
        "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$",
        "a.b@x.io",
        true,
    );
    check(
        "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$",
        "a.b@x.i",
        false,
    );
    check(
        "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$",
        "a@b@x.io",
        false,
    );
    check("^abc$", "abc\n", false);
    check("^a{2,4}$", "a", false);
    check("^a{2,4}$", "aaaa", true);
    check("^a{2,4}$", "aaaaa", false);
    check("^(a|ab)(c|bcd)$", "abcd", true);
    check("^(a*)*b$", &"a".repeat(1000), false);
}

#[test]
fn unanchored_patterns_match_anywhere() {
    check("\\d+", "abc 42 def", true);
    check("\\d+", "abc def", false);
    check("a+b", "xxaaaab", true);
    check("a+b", "xxaaaa", false);
    check("ab{2}c", "abbbc abbc", true);
    check("^https?://", "https://x", true);
    check("^https?://", " http://x", false);
    check("\\.json$", "a.json", true);
    check("\\.json$", "a.json.bak", false);
    check("(\\+)\\w+", "a +b", true);
    check("(\\+)\\w+", "a + b", false);
}

#[test]
fn alternatives_keep_their_own_anchors() {
    check("^(\\d\\.)(\\d)$|^(\\d)$", "1.2", true);
    check("^(\\d\\.)(\\d)$|^(\\d)$", "7", true);
    check("^(\\d\\.)(\\d)$|^(\\d)$", "1.22", false);
    check("^de+v|al+pha|sta+ble$", "devops", true);
    check("^de+v|al+pha|sta+ble$", "an alpha one", true);
    check("^de+v|al+pha|sta+ble$", "unstable", true);
    check("^de+v|al+pha|sta+ble$", "a dev", false);
    check("^de+v|al+pha|sta+ble$", "stables", false);
}

#[test]
fn characters_past_ascii() {
    check("^[à-ÿ]+$", "éè", true);
    check("^[à-ÿ]+$", "éxè", false);
    check("^.\\d$", "😀1", true);
    check("^.\\d$", "\u{2028}1", false);
    check("^\\w+\\s\\w+$", "a\u{3000}b", true);
    check("^\\w+\\S\\w+$", "a\u{3000}b", false);
    check("^[^a]b$", "日b", true);
    check("x[]y|z", "xy", false);
    check("^a?$", "", true);
    check("^a+$", "", false);
}
