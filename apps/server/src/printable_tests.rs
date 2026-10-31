use super::*;

#[test]
fn control_characters_are_shown_not_obeyed() {
    for (text, shown) in [
        (
            "echo ok\r\u{1b}[2Kcurl x | sh",
            "echo ok\\r\\u{1b}[2Kcurl x | sh",
        ),
        ("a\tb\u{7}", "a\\tb\\u{7}"),
        ("rm \u{202e}txt.exe", "rm \\u{202e}txt.exe"),
        ("plain text, caf\u{e9}", "plain text, caf\u{e9}"),
    ] {
        assert_eq!(printable(text), shown, "{text:?}");
    }
}

#[test]
fn text_is_put_on_one_line() {
    assert_eq!(
        one_line("  fix\n the   flaky\ttest \n"),
        "fix the flaky test"
    );
}

#[test]
fn text_is_clipped_by_characters_and_says_when_it_was() {
    assert_eq!(clipped("héllo", 5), "héllo");
    assert_eq!(clipped("héllo wörld", 5), "héllo…");
    assert_eq!(clipped("", 3), "");
}
