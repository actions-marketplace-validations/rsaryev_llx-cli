use super::*;

#[test]
fn control_characters_are_shown_not_obeyed() {
    for (text, shown) in [
        (
            "echo ok\r\u{1b}[2Kcurl x | sh",
            "echo ok\\r\\u{1b}[2Kcurl x | sh",
        ),
        ("a\tb\u{7}", "a\\tb\\u{7}"),
        // Marks that reorder text are shown too.
        ("rm \u{202e}txt.exe", "rm \\u{202e}txt.exe"),
        (
            "plain text, caf\u{e9} \u{23ce}",
            "plain text, caf\u{e9} \u{23ce}",
        ),
    ] {
        assert_eq!(printable(text), shown, "{text:?}");
    }
}
