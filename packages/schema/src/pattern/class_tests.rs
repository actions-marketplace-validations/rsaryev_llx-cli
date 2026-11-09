use super::Class;

#[test]
fn ranges_merge_and_are_found() {
    let mut class = Class::range('a', 'f').unwrap();
    class.union(&Class::range('g', 'k').unwrap());
    class.union(&Class::one('0'));
    assert_eq!(class.char_ranges(), vec![('0', '0'), ('a', 'k')]);
    assert!(class.contains('a') && class.contains('k') && class.contains('0'));
    assert!(!class.contains('l') && !class.contains('1'));
    assert!(Class::range('z', 'a').is_none());
}

#[test]
fn class_escapes_are_ecma_262s() {
    assert!(Class::digit().contains('7'));
    assert!(!Class::digit().contains('\u{0663}'), "\\d is ASCII");
    assert!(Class::word().contains('_') && !Class::word().contains('é'));
    for space in [
        '\t', '\n', '\u{B}', '\u{C}', '\r', ' ', '\u{A0}', '\u{FEFF}', '\u{1680}',
    ] {
        assert!(Class::space().contains(space), "{space:?}");
    }
    for space in [
        '\u{2000}', '\u{200A}', '\u{2028}', '\u{2029}', '\u{202F}', '\u{205F}',
    ] {
        assert!(Class::space().contains(space), "{space:?}");
    }
    assert!(Class::space().contains('\u{3000}'));
    assert!(!Class::space().contains('\u{1}') && !Class::space().contains('\u{2013}'));
    assert!(
        !Class::space().contains('\u{200B}'),
        "a zero-width space is not a space"
    );
}

#[test]
fn the_dot_takes_all_but_line_terminators() {
    let dot = Class::dot();
    for c in ['a', '\u{0}', '\u{85}', '\u{10FFFF}', '😀', '\u{2027}'] {
        assert!(dot.contains(c), "{c:?}");
    }
    for c in ['\n', '\r', '\u{2028}', '\u{2029}'] {
        assert!(!dot.contains(c), "{c:?}");
    }
}

#[test]
fn negation_is_exact_and_skips_surrogates() {
    let not_digit = Class::digit().negated();
    assert!(!not_digit.contains('5') && not_digit.contains('a') && not_digit.contains('\u{0}'));
    assert_eq!(not_digit.negated(), Class::digit());
    assert_eq!(
        Class::empty().negated().char_ranges(),
        vec![('\u{0}', '\u{D7FF}'), ('\u{E000}', '\u{10FFFF}')]
    );
}

#[test]
fn the_ascii_table_and_single() {
    assert_eq!(Class::digit().ascii(), 0x3FF << 48);
    assert!(!Class::word().beyond_ascii() && Class::dot().beyond_ascii());
    assert_eq!(Class::one('x').single(), Some('x'));
    assert_eq!(Class::digit().single(), None);
}

fn property(name: &str) -> Option<Class> {
    Class::ranges(name).map(Class::of)
}

#[test]
fn the_few_properties_known_exactly() {
    let spaces = property("Zs").unwrap();
    assert!(spaces.contains(' ') && spaces.contains('\u{3000}') && spaces.contains('\u{202F}'));
    assert!(!spaces.contains('\t') && !spaces.contains('\u{2028}'));
    assert_eq!(property("General_Category=Space_Separator"), Some(spaces));
    assert!(property("AHex").unwrap().contains('F'));
    assert!(!property("ASCII").unwrap().contains('é'));
    assert_eq!(property("Letter"), None);
}
