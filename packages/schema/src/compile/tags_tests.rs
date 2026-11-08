use serde_json::json;

use crate::Schema;

/// A union of tool calls, each told by the field `type`.
fn calls() -> serde_json::Value {
    let call = |kind: &str, field: &str| {
        json!({"type": "object", "required": ["type", field],
            "properties": {"type": {"const": kind}, field: {"type": "string"}}})
    };
    json!({"$defs": {"read": call("read", "path"), "run": call("run", "command"),
            "ask": call("ask", "question")},
        "oneOf": [{"$ref": "#/$defs/read"}, {"$ref": "#/$defs/run"}, {"$ref": "#/$defs/ask"},
            {"type": "object", "required": ["note"]}]})
}

#[test]
fn a_tagged_union_is_judged_as_its_branches_say() {
    let schema = Schema::new(calls());
    assert!(schema.matches(&json!({"type": "run", "command": "ls"})));
    assert!(
        !schema.matches(&json!({"type": "run", "path": "a"})),
        "run needs its command"
    );
    assert!(!schema.matches(&json!({"type": "fly", "command": "ls"})));
    assert!(
        !schema.matches(&json!({"command": "ls"})),
        "no tag, no tagged branch"
    );
    assert!(
        schema.matches(&json!({"note": "x"})),
        "the untagged branch is still tried"
    );
    assert!(
        !schema.matches(&json!({"type": "run", "command": "ls", "note": 1})),
        "two match"
    );
    assert!(!schema.matches(&json!({"type": 1, "command": "ls"})));
    assert!(
        !schema.matches(&json!("run")),
        "a string matches no object branch here"
    );
}

#[test]
fn a_union_whose_branches_share_no_tag_tries_them_all() {
    let schema = Schema::new(json!({"anyOf": [
        {"required": ["a"], "properties": {"a": {"const": "x"}}},
        {"required": ["b"], "properties": {"b": {"const": "y"}}},
        {"required": ["c"], "properties": {"c": {"enum": ["z", 1]}}}]}));
    assert!(schema.matches(&json!({"b": "y"})));
    assert!(schema.matches(&json!({"c": 1})));
    assert!(!schema.matches(&json!({"c": 2})));
}

#[test]
fn a_tag_with_values_shared_by_branches_tries_each() {
    let schema = Schema::new(json!({"anyOf": [
        {"required": ["k"], "properties": {"k": {"enum": ["a", "b"]}, "n": {"type": "integer"}}},
        {"required": ["k"], "properties": {"k": {"const": "b"}, "n": {"type": "string"}}},
        {"required": ["k"], "properties": {"k": {"const": "c"}}}]}));
    assert!(
        schema.matches(&json!({"k": "b", "n": "s"})),
        "the second branch takes it"
    );
    assert!(schema.matches(&json!({"k": "b", "n": 1})), "the first does");
    assert!(!schema.matches(&json!({"k": "a", "n": "s"})));
}
