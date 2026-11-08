use super::super::parse::parse;
use super::emit;

fn lite(source: &str) -> regex_lite::Regex {
    let emitted = emit(&parse(source).unwrap());
    regex_lite::Regex::new(&emitted).unwrap_or_else(|e| panic!("{source} as {emitted}: {e}"))
}

#[test]
fn classes_are_written_as_code_points() {
    assert_eq!(emit(&parse("a").unwrap()), "[\\x{61}]");
    assert_eq!(emit(&parse("[a-c]").unwrap()), "[\\x{61}-\\x{63}]");
    assert_eq!(emit(&parse("[^a]").unwrap()), "[^\\x{61}]");
    assert_eq!(emit(&parse("[]").unwrap()), "(?:\\b\\B)");
}

#[test]
fn quantifiers_and_assertions() {
    assert_eq!(
        emit(&parse("^a*b+c?d{2}e{2,}f{2,3}\\b\\B$").unwrap()),
        "(?:^(?:[\\x{61}])*(?:[\\x{62}])+(?:[\\x{63}]){0,1}(?:[\\x{64}]){2}(?:[\\x{65}]){2,}\
         (?:[\\x{66}]){2,3}\\b\\B$)"
    );
}

#[test]
fn ecma_meanings_survive_the_translation() {
    // Rust's `\s` is ASCII in regex-lite, ECMA-262's holds Unicode's spaces.
    assert!(lite("^\\s$").is_match("\u{3000}"));
    assert!(!lite("^\\S$").is_match("\u{A0}"));
    // ECMA-262's `.` refuses \r and the Unicode line terminators; Rust's only refuses \n.
    assert!(!lite("^.$").is_match("\r") && !lite("^.$").is_match("\u{2028}"));
    assert!(lite("^.$").is_match("😀"));
    // `\d` and `\w` stay ASCII; `\b` is ASCII's word boundary.
    assert!(!lite("^\\d$").is_match("\u{0663}") && !lite("^\\w$").is_match("é"));
    assert!(lite("\\bcat\\b").is_match("a cat!") && !lite("\\bcat\\b").is_match("bobcat"));
    assert!(
        lite("\\bcole").is_match("écoles"),
        "é is no ASCII word character"
    );
    // `$` is the text's end, never before a final newline.
    assert!(!lite("^abc$").is_match("abc\n"));
    // An empty class matches nothing, an empty group the empty text.
    assert!(!lite("a[]").is_match("a") && lite("a()b").is_match("ab"));
    assert!(lite("^[^]+$").is_match("\n\r😀"));
}
