use super::*;

#[test]
fn a_field_out_of_order_is_still_found() {
    let properties: Vec<(Box<str>, Id, u64)> = ["a", "b", "c", "d"]
        .iter()
        .zip(0..)
        .map(|(name, id)| ((*name).into(), id, 1 << id))
        .collect();
    let mut cursor = Cursor {
        properties: &properties,
        next: 0,
        passed: 0,
        last: "",
    };
    // In order: each where the last was left.
    assert_eq!(cursor.find("a"), Some((0, 1)));
    assert_eq!(cursor.find("c"), Some((2, 4)));
    // Out of order: behind the cursor, and a name it never meets.
    assert_eq!(cursor.find("b"), Some((1, 2)));
    assert_eq!(cursor.find("zz"), None);
    assert_eq!(cursor.find("d"), Some((3, 8)));
    assert_eq!(cursor.find("0"), None);
}

#[test]
fn a_required_field_gone_past_is_confirmed_missing_by_one_lookup() {
    let properties: Vec<(Box<str>, Id, u64)> = ["a", "b", "c"]
        .iter()
        .zip(0..)
        .map(|(name, id)| ((*name).into(), id, 1 << id))
        .collect();
    let mut cursor = Cursor {
        properties: &properties,
        next: 0,
        passed: 0,
        last: "",
    };
    assert_eq!(cursor.find("c"), Some((2, 4)));
    assert_eq!(cursor.passed, 0b011, "a and b went past unmet");
    let fields = |names: &[&str]| -> Map<String, Value> {
        names
            .iter()
            .map(|name| ((*name).to_string(), Value::Null))
            .collect()
    };
    // "a" came out of order: it is there; "b" is not.
    assert!(cursor.missing(&fields(&["c", "a"]), 0));
    assert_eq!(cursor.passed, 0, "what was asked is not asked again");
    cursor.passed = 0b011;
    assert!(!cursor.missing(&fields(&["a", "b", "c"]), 0));
}

#[test]
fn names_are_ordered_as_their_bytes_are() {
    use std::cmp::Ordering::{Equal, Greater, Less};
    for (a, b) in [
        ("a", "b"),
        ("ab", "abc"),
        ("field_09", "field_10"),
        ("", "x"),
        ("abcdefgh", "abcdefgi"),
        ("abcdefghij", "abcdefgh"),
        ("é", "z"),
        ("same-name-here", "same-name-here"),
    ] {
        assert_eq!(order(a, b), a.cmp(b), "{a} {b}");
        assert_eq!(order(b, a), b.cmp(a), "{b} {a}");
    }
    assert_eq!(
        (order("x", "x"), order("a", "b"), order("b", "a")),
        (Equal, Less, Greater)
    );
}
