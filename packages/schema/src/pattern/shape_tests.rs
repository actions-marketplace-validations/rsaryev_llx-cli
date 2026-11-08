use super::super::parse::parse;
use super::{branches, nullable};

/// Each branch as (start, end, how many items).
fn shape(source: &str) -> Option<Vec<(bool, bool, usize)>> {
    let node = parse(source).unwrap();
    branches(&node).map(|all| {
        all.iter()
            .map(|branch| (branch.start, branch.end, branch.body.len()))
            .collect()
    })
}

#[test]
fn anchors_at_an_alternatives_ends_are_taken_off() {
    assert_eq!(shape("^abc$"), Some(vec![(true, true, 3)]));
    assert_eq!(shape("abc"), Some(vec![(false, false, 3)]));
    assert_eq!(
        shape("^dev|alpha|stable$"),
        Some(vec![(true, false, 3), (false, false, 5), (false, true, 6)])
    );
    assert_eq!(
        shape("(^a|b$)"),
        Some(vec![(true, false, 1), (false, true, 1)])
    );
    assert_eq!(shape("^(a|b)$"), Some(vec![(true, true, 1)]));
    assert_eq!(shape("^^a$$"), Some(vec![(true, true, 1)]));
    assert_eq!(shape("^$"), Some(vec![(true, true, 0)]));
}

#[test]
fn an_assertion_elsewhere_has_no_shape() {
    for source in ["\\bword", "a^b", "a$b", "^(?:^a|b)", "$^", "(a$)+"] {
        assert_eq!(shape(source), None, "{source}");
    }
}

#[test]
fn nullability() {
    for (source, empty) in [
        ("a*", true),
        ("a?b?", true),
        ("a|", true),
        ("(a|b)+", false),
        ("a{0}", true),
        ("(a*)+", true),
        ("[]", false),
        ("^", true),
    ] {
        assert_eq!(nullable(&parse(source).unwrap()), empty, "{source}");
    }
}
