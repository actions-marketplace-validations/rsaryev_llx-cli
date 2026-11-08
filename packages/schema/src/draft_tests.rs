use serde_json::json;

use super::*;

#[test]
fn a_document_is_read_by_the_draft_it_names_and_2020_12_by_default() {
    let of = |named: &str| Draft::of(&json!({"$schema": named}));
    assert_eq!(of("http://json-schema.org/draft-04/schema#"), Draft::Four);
    assert_eq!(of("http://json-schema.org/draft-07/schema"), Draft::Seven);
    assert_eq!(
        of("https://json-schema.org/draft/2019-09/schema"),
        Draft::Nineteen
    );
    assert_eq!(Draft::of(&json!({"type": "object"})), Draft::Twenty);
    assert!(!Draft::Four.knows("if") && Draft::Seven.knows("if"));
    assert!(Draft::Seven.knows("dependencies") && !Draft::Twenty.knows("dependencies"));
    assert!(!Draft::Nineteen.knows("prefixItems") && Draft::Twenty.knows("prefixItems"));
    assert!(Draft::Four.knows("properties"));
}
