use super::*;

#[test]
fn a_short_label_is_folded_as_any_label_is() {
    let (whole, short) = ("a B\tc ".repeat(11), "a B\tc ".repeat(11));
    let (whole, short) = (whole.split_at(SHORT).0, short.split_at(SHORT - 1).0);
    let labels = [
        "x",
        "Foo   BAR",
        " \t lead and trail \n ",
        "a\u{b}b\u{c}c\rd\ne",
        "",
        "   ",
        "snake_case-and.dots[1]",
        whole,
        short,
    ];
    for label in labels {
        let mut short = [0; SHORT];
        assert_eq!(
            folded(label, &mut short),
            Some(normalize(label).as_str()),
            "{label:?}"
        );
    }
}

#[test]
fn a_long_label_and_one_past_ascii_are_left_to_the_slow_way() {
    let mut short = [0; SHORT];
    assert_eq!(folded(&"a".repeat(SHORT + 1), &mut short), None);
    assert_eq!(folded("Straße", &mut short), None);
}

#[test]
fn a_link_finds_its_definition_whatever_the_case_and_the_spaces() {
    let mut refs = Refs::default();
    let mut own = String::new();
    assert!(refs.define("[Foo  Bar]: /there\n", &mut own, 0).is_some());
    assert!(refs.define("[ΑΓΩ]: /greek\n", &mut own, 0).is_some());
    for label in ["foo bar", "FOO\tBAR", " foo\n bar ", "αγω"] {
        assert!(refs.get(label).is_some(), "{label:?}");
    }
    assert!(refs.get("foobar").is_none());
}
