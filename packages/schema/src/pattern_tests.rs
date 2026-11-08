use super::{Engine, Kind, Pattern, lite, parse};

/// The engine a pattern is checked by.
fn engine(source: &str) -> &'static str {
    match Pattern::new(source).unwrap().engine {
        Engine::Simple(_) => "simple",
        Engine::Narrow(_)
        | Engine::Wide(_)
        | Engine::Wider(_)
        | Engine::Widest(_)
        | Engine::Table(_) => "automaton",
        Engine::Lite(_) => "lite",
    }
}

/// The answer of the pattern's own engine, checked against `regex-lite` given the same pattern.
fn check(source: &str, text: &str, expected: bool) {
    let pattern = Pattern::new(source).unwrap_or_else(|e| panic!("{source}: {e}"));
    assert_eq!(pattern.is_match(text), expected, "{source} on {text:?}");
    let Engine::Lite(regex) = lite(&parse::parse(source).unwrap()).unwrap() else {
        panic!("not lite")
    };
    assert_eq!(
        regex.is_match(text),
        expected,
        "{source} on {text:?} (regex-lite)"
    );
}

#[test]
fn each_pattern_takes_the_cheapest_engine() {
    for (source, expected) in [
        ("^[A-Z]{3}$", "simple"),
        ("^x-", "simple"),
        ("^(small|medium|large)$", "simple"),
        (".*", "simple"),
        ("^\\d{4}-\\d{2}-\\d{2}$", "automaton"),
        ("\\d+", "simple"),
        (
            "^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
            "automaton",
        ),
        ("^(a{1,100}b){2}$", "automaton"),
        ("^(a{1,100}b){3}$", "automaton"),
        ("^(a{1,100}b){6}$", "automaton"),
        ("^(a{1,1000}b){5}$", "lite"),
        ("a[ab]{600}$", "lite"),
        ("\\bword\\b", "automaton"),
        ("\\Bx", "automaton"),
        ("^\\w+\\b-\\b\\w+$", "automaton"),
        ("a[ab]{14}\\b", "lite"),
        (
            "^https?:\\/\\/(www\\.)?[-a-zA-Z0-9@:%._\\+~#=]{1,256}\\.[a-zA-Z0-9()]{1,6}\\b([-a-zA-Z0-9()@:%_\\+.~#?&//=]*)$",
            "automaton",
        ),
        ("a^b", "automaton"),
        ("[a-zA-Z0-9_-]{162}", "simple"),
        ("[a-z0-9]{2}x", "automaton"),
        ("^$|(^(?:\\S+\\s+){0,99}\\S+$)", "automaton"),
        ("^([^,]|$)", "automaton"),
    ] {
        assert_eq!(engine(source), expected, "{source}");
    }
}

#[test]
fn typical_patterns() {
    check("^[A-Z]{3}$", "USD", true);
    check("^[A-Z]{3}$", "USDT", false);
    check("^[a-z0-9_-]{3,16}$", "my-name_1", true);
    check(
        "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$",
        "jo@ex.com",
        true,
    );
    check(
        "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$",
        "jo@excom",
        false,
    );
    check("^https?://", "http://a", true);
    check("^(small|medium|large)$", "large", true);
    check("^(small|medium|large)$", "huge", false);
    check("\\d+", "v2", true);
    check("^(\\+\\d{1,3})?\\d{10}$", "+15551234567", true);
    check(
        "^[1-9][0-9]{3}-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[0-1])$",
        "2024-13-01",
        false,
    );
    check("^(?:\\S+\\s+){0,9}\\S+$", "one two three", true);
    check("^(?:\\S+\\s+){0,9}\\S+$", "1 2 3 4 5 6 7 8 9 10 11", false);
}

#[test]
fn the_match_is_unanchored_unless_anchored() {
    check("a+", "xxaayy", true);
    check("f.*o", "a foo", true);
    check("[0-9]{2,}", "answer 1", false);
    check("[0-9]{2,}", "a31b", true);
    check("X_", "a_x_3", false);
    check("^a*$", "abc", false);
    check("^abc$", "abc\n", false);
    check("abc$", "xabc", true);
}

#[test]
fn ecma_262_differs_from_rust_where_it_should() {
    check("^\\t$", "\t", true);
    check("^\\t$", "\\t", false);
    check("^\\cC$", "\u{3}", true);
    check("^\\d$", "\u{07C0}", false);
    check("^\\D$", "\u{07C0}", true);
    check("^\\w$", "é", false);
    check("^\\W$", "é", true);
    for space in [
        " ", "\t", "\u{B}", "\u{C}", "\u{A0}", "\u{FEFF}", "\n", "\u{2029}", "\u{2003}",
    ] {
        check("^\\s$", space, true);
        check("^\\S$", space, false);
    }
    check("^\\s$", "\u{2013}", false);
    check("^.$", "\r", false);
    check("^.$", "\u{85}", true);
    check("\\wcole", "l'école", false);
    check("[a-z]cole", "l'ecole", true);
}

#[test]
fn word_boundaries_are_ascii() {
    check("\\bcat\\b", "the cat sat", true);
    check("\\bcat\\b", "concatenate", false);
    check("\\Bcat", "concat", true);
    check("^\\w+\\b", "héllo", true);
}

#[test]
fn unicode_is_matched_by_character() {
    check("^.{3}$", "日本語", true);
    check("^.{3}$", "日本", false);
    check("^[😀-😂]+$", "😁😀", true);
    check("^\\u{1F600}$", "😀", true);
    check("^\\uD83D\\uDE00$", "😀", true);
    check("é+", "caféé", true);
}

#[test]
fn a_long_text_is_matched_in_linear_time() {
    let text = "a".repeat(100_000);
    check("^(a+)+$", &text, true);
    check("^(a+)+b$", &text, false);
    check("(a|aa)*c", &text, false);
    check("\\b(a|aa)*c", &text, false);
}

#[test]
fn what_is_not_checked_says_why() {
    let error = |source: &str| Pattern::new(source).err().unwrap();
    assert_eq!(error("(a").kind, Kind::Invalid);
    assert!(
        error("(a")
            .to_string()
            .starts_with("not an ECMA-262 regular expression: ")
    );
    assert_eq!(error("\\p{Letter}").kind, Kind::Unchecked);
    assert_eq!(
        error("\\p{Letter}").to_string(),
        "the Unicode property `Letter` is not checked"
    );
    assert_eq!(error("(?=a)").kind, Kind::Unchecked);
    assert_eq!(error("(a)\\1").kind, Kind::Unchecked);
}

#[test]
fn a_property_known_exactly_is_checked() {
    check("^[\\p{Zs}A-Za-z]+$", "a\u{3000}b", true);
    check("^[\\p{Zs}A-Za-z]+$", "a\tb", false);
    check("^\\P{ASCII}$", "é", true);
}

#[test]
fn a_pattern_past_512_positions_runs_as_a_dfa_when_it_has_one() {
    let words: Vec<String> = (0..300).map(|n| format!("{n}x{}", 300 - n)).collect();
    let source = format!("^u?fixed({})?$", words.join("|"));
    assert_eq!(engine(&source), "automaton");
    for (text, expected) in [
        ("fixed", true),
        ("ufixed0x300", true),
        ("fixed150x150", true),
        ("fixed299x1", true),
        ("fixed150x151", false),
        ("fixed0x300 ", false),
        ("ufixe", false),
        ("fixed\u{e9}", false),
    ] {
        check(&source, text, expected);
    }
}

#[test]
fn a_word_boundary_is_seen_between_the_characters_on_its_sides() {
    for (source, text, expected) in [
        ("\\bfoo\\b", "foo", true),
        ("\\bfoo\\b", "a foo.", true),
        ("\\bfoo\\b", "foobar", false),
        ("\\bfoo\\b", "_foo", false),
        ("\\bfoo\\b", "\u{e9}foo\u{e9}", true),
        ("\\Bx", "x", false),
        ("\\Bx", "ax", true),
        ("\\Bx", " x", false),
        ("\\Bx", "\u{e9}x", false),
        ("x\\B", "xa", true),
        ("x\\B", "x ", false),
        ("x\\B", "x", false),
        ("^\\w+\\b-\\b\\w+$", "ab-cd", true),
        ("^\\w+\\b-\\b\\w+$", "ab--cd", false),
        ("\\d\\b", "12 ", true),
        ("\\d\\b", "12a", false),
        ("\\d\\b", "1", true),
        ("a\\b|b", "ac", false),
        ("a\\b|b", "acb", true),
        ("(\\b|x)y", "ay", false),
        ("(\\b|x)y", "axy", true),
        ("(\\b|x)y", "a y", true),
        ("(^|\\b)x", "ax", false),
        ("(^|\\b)x", "a x", true),
        ("\\b\u{65e5}", "\u{65e5}", false),
        ("\\b\u{65e5}", "a\u{65e5}", true),
        ("\\b\\B", "a", false),
        ("a\\b\\Bb", "ab", false),
        ("(a\\b|\\Bb)+c", "abc", true),
        ("(a\\b|\\Bb)+c", "a c", false),
        ("(a|b\\b)+ c", "abab c", true),
        ("\\b[a-z]+\\b \\b[0-9]+\\b", "x abc 123 y", true),
        ("\\b[a-z]+\\b \\b[0-9]+\\b", "x abc 123y", false),
    ] {
        check(source, text, expected);
    }
}

#[test]
fn a_boundary_with_no_character_around_it_looks_at_the_text_s_ends() {
    for (source, text, expected) in [
        ("\\b", "", false),
        ("\\b", " ", false),
        ("\\b", " a", true),
        ("\\B", "", true),
        ("\\B", "a", false),
        ("\\B", "ab", true),
        ("\\B", "a-a", false),
        ("\\B", " ", true),
        ("^\\b", "a", true),
        ("^\\b", " a", false),
        ("^\\b", "", false),
        ("^\\B", "", true),
        ("^\\B", " a", true),
        ("^\\B", "a", false),
        ("\\b$", "a", true),
        ("\\b$", "a ", false),
        ("\\B$", "a ", true),
        ("\\B$", "a", false),
        ("^\\B$", "", true),
        ("^\\b$", "", false),
        ("x|\\b$", "a", true),
        ("x|^\\b", " ", false),
    ] {
        check(source, text, expected);
    }
}
