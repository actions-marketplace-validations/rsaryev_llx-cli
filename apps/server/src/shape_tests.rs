use serde_json::json;

use super::*;

#[test]
fn what_the_check_cannot_hold_data_to_is_said_once_as_the_run_starts() {
    let schema = Checked::new(json!({"type": "object", "properties": {
        "id": {"type": "string", "pattern": "^(?!x)[a-z]+$"},
        "at": {"type": "string", "format": "idn-email"},
        "tag": {"pattern": "(?=y)"},
        "day": {"type": "string", "format": "date", "pattern": "^[0-9-]+$"}
    }}));
    let note = schema.unchecked_note().unwrap();
    assert!(
        note.starts_with("llx: --schema: format, pattern are not checked (the model is told them;"),
        "{note}"
    );
    let one = Checked::new(json!({"pattern": "(a)\\1"}))
        .unchecked_note()
        .unwrap();
    assert!(
        one.starts_with("llx: --schema: pattern is not checked (the model is told it;"),
        "{one}"
    );
    assert_eq!(
        Checked::new(json!({"type": "object"})).unchecked_note(),
        None
    );
    // A field named as a keyword is a field.
    assert_eq!(
        Checked::new(json!({"properties": {"format": {"type": "string"}}})).unchecked_note(),
        None
    );
}

#[test]
fn the_agent_is_told_the_schema_and_every_way_data_breaks_it() {
    let schema = Checked::new(
        json!({"type": "object", "required": ["n"], "properties": {"n": {"type": "integer", "minimum": 1}}}),
    );
    assert_eq!(schema.schema()["required"], json!(["n"]));
    assert!(schema.mismatches(&json!({"n": 3})).is_empty());
    assert_eq!(
        schema.mismatches(&json!({"n": 0})),
        ["data.n must be >= 1 (is 0)"]
    );
}
