use serde_json::json;

use crate::compile::compile;
use crate::node::{BOOLEAN, INTEGER, Items, NULL, NUMBER, Node, STRING};

/// The types the root of `schema` was given as its own, if any.
fn folded(schema: &serde_json::Value) -> Option<u8> {
    match compile(schema, &[]).nodes.first() {
        Some(Node::Rules(rules)) => rules.only_types,
        Some(Node::Any | Node::Never) | None => None,
    }
}

#[test]
fn a_union_of_types_alone_takes_their_types() {
    let text_or_number = json!({"anyOf": [{"type": "string"}, {"type": "number"}]});
    assert_eq!(folded(&text_or_number), Some(STRING | NUMBER | INTEGER));
    let through_refs = json!({"$defs": {"t": {"$ref": "#/$defs/u"}, "u": {"type": "null"}},
        "oneOf": [{"$ref": "#/$defs/t"}, {"type": "boolean"}]});
    assert_eq!(folded(&through_refs), Some(NULL | BOOLEAN));
    let nested = json!({"anyOf": [{"anyOf": [{"type": "null"}]}, {"type": "string"}]});
    assert_eq!(folded(&nested), Some(NULL | STRING));
    let narrowed = json!({"type": "integer", "anyOf": [{"type": "number"}, {"type": "string"}]});
    assert_eq!(folded(&narrowed), Some(INTEGER));
}

#[test]
fn a_union_that_asks_more_than_types_is_walked() {
    // Two branches of a `oneOf` that share a type can both match one value.
    assert_eq!(
        folded(&json!({"oneOf": [{"type": "integer"}, {"type": "number"}]})),
        None
    );
    assert_eq!(
        folded(&json!({"anyOf": [{"type": "string", "minLength": 2}, {"type": "null"}]})),
        None
    );
    assert_eq!(
        folded(&json!({"anyOf": [{"type": "null"}], "minimum": 1})),
        None
    );
    assert_eq!(
        folded(&json!({"anyOf": [{"type": "null"}], "not": {"type": "null"}})),
        None
    );
}

#[test]
fn a_ref_and_nothing_else_takes_the_types_it_lands_on() {
    let schema = json!({"$defs": {"a": {"$ref": "#/$defs/b"}, "b": {"type": "string"},
            "any": {"$ref": "#/$defs/all"}, "all": true},
        "items": [{"$ref": "#/$defs/a"}, {"$ref": "#/$defs/any"}]});
    let nodes = compile(&schema, &[]).nodes;
    let types: Vec<Option<u8>> = (nodes.iter())
        .map(|node| match node {
            Node::Rules(rules) => rules.only_types,
            Node::Any | Node::Never => None,
        })
        .collect();
    assert!(types.contains(&Some(STRING)), "{types:?}");
    assert!(types.contains(&Some(0x7F)), "{types:?}");
}

#[test]
fn an_array_of_typed_items_with_a_count_is_planned_and_judged_as_walked() {
    let plain = |schema: &serde_json::Value| match compile(schema, &[]).nodes.first() {
        Some(Node::Rules(rules)) => rules
            .plain_array
            .as_ref()
            .map(|plan| (plan.min, plan.max, plan.items)),
        Some(Node::Any | Node::Never) | None => None,
    };
    let pair = json!({"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}});
    assert_eq!(plain(&pair), Some((2, 2, Items::Types(NUMBER | INTEGER))));
    let matrix = json!({"type": "array", "items": pair});
    assert!(matches!(
        plain(&matrix),
        Some((0, u64::MAX, Items::Plain(_)))
    ));
    let rows = crate::Schema::new(matrix);
    assert!(rows.matches(&json!([[1, 2], [3, 4.5]])));
    assert!(!rows.matches(&json!([[1, 2], [3]])));
    let looped = json!({"$defs": {"n": {"type": "array", "items": {"$ref": "#/$defs/n"}}}, "$ref": "#/$defs/n"});
    assert!(crate::Schema::new(looped).matches(&json!([[], [[]]])));
    assert_eq!(plain(&json!({"type": "array", "uniqueItems": true})), None);
    assert_eq!(
        plain(&json!({"items": {"type": "string", "minLength": 1}})),
        None
    );
    let schema = crate::Schema::new(pair);
    assert!(schema.matches(&json!([1, 2.5])));
    assert!(!schema.matches(&json!([1])));
    assert!(!schema.matches(&json!([1, "a"])));
    assert!(!schema.matches(&json!("a")));
    assert_eq!(
        schema.mismatches(&json!([1, "a"])),
        ["data[1] must be number (is string)"]
    );
}
