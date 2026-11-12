use super::*;

fn decoded(text: &str) -> Option<(String, usize)> {
    let mut out = String::new();
    entity(text.as_bytes(), 0, &mut out).map(|end| (out, end))
}

#[test]
fn a_named_reference_is_its_characters() {
    assert_eq!(decoded("&amp; x"), Some(("&".into(), 5)));
    assert_eq!(decoded("&AElig;"), Some(("Æ".into(), 7)));
    assert_eq!(decoded("&zwnj;"), Some(("\u{200c}".into(), 6)));
    assert_eq!(decoded("&nGt;"), Some(("\u{226b}\u{20d2}".into(), 5)));
    assert_eq!(decoded("&AElig"), None, "no semicolon");
    assert_eq!(decoded("&nosuch;"), None);
    assert_eq!(decoded("&;"), None);
}

#[test]
fn a_numeric_reference_is_its_code_point() {
    assert_eq!(decoded("&#35;"), Some(("#".into(), 5)));
    assert_eq!(decoded("&#X22;"), Some(("\"".into(), 6)));
    assert_eq!(decoded("&#0;"), Some(("\u{fffd}".into(), 4)));
    assert_eq!(decoded("&#xD800;"), Some(("\u{fffd}".into(), 8)));
    assert_eq!(decoded("&#87654321;"), None, "too many digits");
    assert_eq!(decoded("&#;"), None);
}

#[test]
fn escapes_and_references_are_decoded_in_a_string() {
    let mut out = String::new();
    unescape(r"a\*b &copy; \q &nope; é", &mut out);
    assert_eq!(out, r"a*b © \q &nope; é");
}
