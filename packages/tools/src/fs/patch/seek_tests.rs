use super::*;

fn lines(all: &[&str]) -> Vec<String> {
    all.iter().map(ToString::to_string).collect()
}

#[test]
fn the_exact_lines_are_found_after_the_start() {
    let file = ["a", "b", "a", "b"];
    assert_eq!(seek(&file, &lines(&["a", "b"]), 0, false), Some(0));
    assert_eq!(seek(&file, &lines(&["a", "b"]), 1, false), Some(2));
    assert_eq!(seek(&file, &lines(&["b", "c"]), 0, false), None);
}

#[test]
fn whitespace_and_look_alike_punctuation_are_forgiven_in_that_order() {
    let file = ["  let x = 1;   ", "y"];
    // Trailing whitespace, then surrounding whitespace.
    assert_eq!(seek(&file, &lines(&["  let x = 1;"]), 0, false), Some(0));
    assert_eq!(seek(&file, &lines(&["let x = 1;"]), 0, false), Some(0));
    let typed = ["say \u{201C}hi\u{201D} \u{2013} it\u{2019}s"];
    assert_eq!(
        seek(&typed, &lines(&["say \"hi\" - it's"]), 0, false),
        Some(0)
    );
    // An exact match further down wins over a looser one above it.
    let file = [" a", "a"];
    assert_eq!(seek(&file, &lines(&["a"]), 0, false), Some(1));
}

#[test]
fn an_end_of_file_pattern_is_looked_for_at_the_end_first() {
    let file = ["x", "y", "x"];
    assert_eq!(seek(&file, &lines(&["x"]), 0, true), Some(2));
    assert_eq!(seek(&file, &lines(&["x"]), 0, false), Some(0));
    // Not at the end: found elsewhere all the same.
    assert_eq!(seek(&file, &lines(&["y"]), 0, true), Some(1));
}

#[test]
fn an_empty_pattern_matches_where_it_starts_and_a_longer_one_nowhere() {
    let file = ["a"];
    assert_eq!(seek(&file, &[], 1, false), Some(1));
    assert_eq!(seek(&file, &lines(&["a", "b"]), 0, false), None);
    assert_eq!(seek(&file, &lines(&["a"]), 5, false), None);
}
