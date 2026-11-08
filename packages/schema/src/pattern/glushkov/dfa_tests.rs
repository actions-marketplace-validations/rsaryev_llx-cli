use super::super::super::parse::parse;
use super::super::Automaton;
use super::decode;

fn automaton(source: &str) -> Automaton<[u64; 4]> {
    Automaton::new(&parse(source).unwrap())
}

/// The DFA's answer, checked against the bit-parallel walk's.
fn check(source: &str, text: &str, expected: bool) {
    let automaton = automaton(source);
    let dfa = automaton.dfa.as_ref().expect("a DFA");
    assert_eq!(dfa.is_match(text), expected, "{source} on {text:?}");
    assert_eq!(
        automaton.parallel(text),
        expected,
        "{source} on {text:?} (bit-parallel)"
    );
}

#[test]
fn a_long_run_is_a_state_a_count() {
    let source = "[a-zA-Z0-9_-]{162}";
    let long = "x".repeat(162);
    check(source, &long, true);
    check(source, &format!("!!{long}!!"), true);
    check(
        source,
        &format!("{}!{}", "x".repeat(100), "x".repeat(100)),
        false,
    );
    let states = automaton(source).dfa.unwrap().states.len();
    assert!(states <= 170, "{states} states");
}

#[test]
fn anchors_inside_groups() {
    let source = "^$|(^(?:\\S+\\s+){0,99}\\S+$)";
    check(source, "", true);
    check(source, "one two three", true);
    check(source, " one", false);
    check(source, "one ", false);
    check(source, "word ".repeat(100).trim_end(), true);
    check(source, "word ".repeat(101).trim_end(), false);
}

#[test]
fn unanchored_patterns_restart_at_every_character() {
    check("\\d+", "abc 42", true);
    check("\\d+", "abc", false);
    check("ab{2}c", "abbbc abbc", true);
    check("(\\+)\\w+", "a + b", false);
    check("\\.json$", "a.json.bak", false);
    check("\\.json$", "a.json", true);
}

#[test]
fn a_character_past_ascii_steps_from_the_state_and_comes_back() {
    check("^[à-ÿ]+x$", "éèx", true);
    check("^[à-ÿ]+x$", "éè", false);
    check("^.\\d+$", "😀12", true);
    check("^\\w+\\s\\w+$", "a\u{3000}b", true);
    check("a.b", "xa😀b", true);
    check("^a[^b]*c$", "a日本c", true);
    check("^a[^b]*c$", "a日b本c", false);
}

#[test]
fn too_many_states_leave_the_bit_parallel_walk() {
    // The n-th character from the end being an `a` takes 2^n states.
    let automaton = automaton("a[ab]{14}$");
    assert!(automaton.dfa.is_none());
    assert!(automaton.is_match(&format!("bbb{}", "a".repeat(15))));
    assert!(!automaton.is_match(&format!("a{}", "b".repeat(15))));
}

#[test]
fn a_character_past_ascii_is_decoded_from_its_bytes() {
    for c in ['é', 'ß', '日', '\u{FFFF}', '😀', '\u{10FFFF}'] {
        let text = format!("{c}x");
        assert_eq!(
            decode(text.as_bytes()),
            Some((u32::from(c), c.len_utf8())),
            "{c:?}"
        );
    }
}
