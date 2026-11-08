use serde_json::json;

use crate::Schema;
use crate::compile::compile;
use crate::node::Node;

/// Whether the root of `schema` was found to take every field, and every item.
fn settled(schema: &serde_json::Value) -> (bool, bool) {
    match compile(schema, &[]).nodes.first() {
        Some(Node::Rules(rules)) => (rules.every_field_seen, rules.every_item_seen),
        Some(Node::Any | Node::Never) | None => (false, false),
    }
}

#[test]
fn what_every_allowed_value_applies_settles_the_unevaluated() {
    let common = json!({"$defs": {"common": {"allOf": [{"additionalProperties": true}]}},
        "allOf": [{"$ref": "#/$defs/common"}], "unevaluatedProperties": false});
    assert_eq!(settled(&common), (true, false));
    let nested = json!({"allOf": [{"unevaluatedItems": true}], "unevaluatedItems": false});
    assert_eq!(settled(&nested), (false, true));
    let own = json!({"items": {"type": "string"}, "unevaluatedItems": false});
    assert_eq!(settled(&own), (false, true));
}

#[test]
fn what_only_some_values_apply_is_worked_out_as_the_data_comes() {
    let branch = json!({"anyOf": [{"additionalProperties": true}, {"type": "string"}],
        "unevaluatedProperties": false});
    assert_eq!(settled(&branch), (false, false));
    let itself = json!({"unevaluatedProperties": false});
    assert_eq!(settled(&itself), (false, false));
    let looped = json!({"$defs": {"a": {"$ref": "#/$defs/a"}}, "allOf": [{"$ref": "#/$defs/a"}],
        "unevaluatedProperties": false});
    assert_eq!(settled(&looped), (false, false));
    assert!(!Schema::new(looped).matches(&json!({"x": 1})));
    assert!(Schema::new(branch).matches(&json!({"x": 1})));
    let named = json!({"allOf": [{"properties": {"a": true}}], "unevaluatedProperties": false});
    assert!(Schema::new(named.clone()).matches(&json!({"a": 1})));
    assert!(!Schema::new(named).matches(&json!({"a": 1, "b": 2})));
}
