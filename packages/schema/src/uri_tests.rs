use super::*;

#[test]
fn references_resolve_as_rfc_3986_says() {
    // RFC 3986 section 5.4.
    let base = "http://a/b/c/d;p?q";
    for (reference, resolved) in [
        ("g", "http://a/b/c/g"),
        ("./g", "http://a/b/c/g"),
        ("g/", "http://a/b/c/g/"),
        ("/g", "http://a/g"),
        ("//g", "http://g"),
        ("?y", "http://a/b/c/d;p?y"),
        ("g?y", "http://a/b/c/g?y"),
        ("#s", "http://a/b/c/d;p?q#s"),
        ("", "http://a/b/c/d;p?q"),
        (".", "http://a/b/c/"),
        ("..", "http://a/b/"),
        ("../g", "http://a/b/g"),
        ("../../g", "http://a/g"),
        ("../../../g", "http://a/g"),
        ("g;x=1/../y", "http://a/b/c/y"),
        ("urn:x:y", "urn:x:y"),
    ] {
        assert_eq!(resolve(base, reference), resolved, "{reference}");
    }
}

#[test]
fn a_document_with_no_id_has_the_default_base() {
    assert_eq!(resolve(BASE, "#/$defs/a"), "json-schema:///#/$defs/a");
    assert_eq!(resolve(BASE, "item.json"), "json-schema:///item.json");
    assert_eq!(split("http://x/y.json#/a"), ("http://x/y.json", "/a"));
    assert_eq!(split("http://x/y.json"), ("http://x/y.json", ""));
}
