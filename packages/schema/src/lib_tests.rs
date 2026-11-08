use serde_json::json;

use super::*;

#[expect(
    clippy::needless_pass_by_value,
    reason = "a test helper takes its arguments as the tests write them, by value"
)]
fn problems(schema: Value, data: Value) -> Vec<String> {
    let schema = Schema::new(schema);
    let found = schema.mismatches(&data);
    assert_eq!(
        schema.matches(&data),
        found.is_empty(),
        "quick and full disagree on {data}"
    );
    found
}

#[test]
fn checks_types_required_enums_and_nesting() {
    let schema = json!({"type": "object", "required": ["price", "date"], "properties": {
        "price": {"type": "number"}, "date": {"type": "string"},
        "stops": {"type": ["integer", "null"]}, "class": {"enum": ["economy", "business"]},
        "legs": {"type": "array", "items": {"type": "object", "required": ["from"], "properties": {"from": {"type": "string"}}}}
    }});
    let fine = json!({"price": 93, "date": "2026-11-27", "stops": null, "class": "economy", "legs": [{"from": "LCA"}]});
    assert!(problems(schema.clone(), fine).is_empty());
    assert_eq!(
        problems(
            schema,
            json!({"price": "93", "stops": 1.5, "class": "first", "legs": [{}]})
        ),
        [
            "data is missing \"date\"",
            "data.class must be one of [\"economy\",\"business\"] (is \"first\")",
            "data.legs[0] is missing \"from\"",
            "data.price must be number (is string)",
            "data.stops must be integer or null (is number)",
        ]
    );
}

#[test]
fn numbers_are_equal_by_value_and_an_integer_has_no_fraction() {
    // JSON Schema's equality: `1` is `1.0`, in `enum`, `const` and `uniqueItems`.
    assert!(problems(json!({"enum": [1, 2]}), json!(1.0)).is_empty());
    assert!(problems(json!({"const": {"a": [1]}}), json!({"a": [1.0]})).is_empty());
    assert_eq!(
        problems(json!({"uniqueItems": true}), json!([1, "x", 1.0])),
        ["data must have unique items (data[0] and data[2] are equal)"]
    );
    assert!(problems(json!({"type": "integer"}), json!(3.0)).is_empty());
    assert_eq!(
        problems(json!({"type": "integer"}), json!(3.5)),
        ["data must be integer (is number)"]
    );
}

#[test]
fn bounds_say_the_limit_and_what_the_data_has() {
    let schema = json!({"type": "object", "properties": {
        "line": {"type": "integer", "minimum": 1, "maximum": 10},
        "score": {"exclusiveMinimum": 0, "exclusiveMaximum": 1},
        "title": {"minLength": 3, "maxLength": 5},
        "tags": {"minItems": 1, "maxItems": 2},
        "meta": {"minProperties": 1, "maxProperties": 1}
    }});
    let data = json!({"line": 0, "score": 1, "title": "日本語の題名", "tags": [], "meta": {}});
    assert_eq!(
        problems(schema, data),
        [
            "data.line must be >= 1 (is 0)",
            "data.meta must have at least 1 fields (has 0)",
            "data.score must be < 1 (is 1)",
            "data.tags must have at least 1 items (has 0)",
            "data.title must have at most 5 characters (has 6)",
        ]
    );
    // A length counts characters, not bytes.
    assert!(problems(json!({"maxLength": 3}), json!("日本語")).is_empty());
}

#[test]
fn a_multiple_is_exact_in_decimal() {
    // The floats' quotient says 0.0075 / 0.0001 is 74.99999999999999.
    assert!(problems(json!({"multipleOf": 0.0001}), json!(0.0075)).is_empty());
    assert!(problems(json!({"multipleOf": 0.1}), json!(0.3)).is_empty());
    assert!(problems(json!({"multipleOf": 5}), json!(25)).is_empty());
    assert_eq!(
        problems(json!({"multipleOf": 0.01}), json!(0.075)),
        ["data must be a multiple of 0.01 (is 0.075)"]
    );
    assert_eq!(
        problems(json!({"multipleOf": 2}), json!(7)),
        ["data must be a multiple of 2 (is 7)"]
    );
    assert!(
        problems(json!({"multipleOf": -1}), json!(i64::MIN)).is_empty(),
        "no overflow"
    );
}

#[test]
fn fields_the_schema_does_not_allow_are_named() {
    let schema = json!({"properties": {"a": {}}, "additionalProperties": false, "dependentRequired": {"a": ["b"]}});
    assert_eq!(
        problems(schema, json!({"a": 1, "z": 2})),
        [
            "data has \"z\", which the schema does not allow",
            "data has \"a\", so it must have \"b\""
        ]
    );
    let typed = json!({"properties": {"a": {}}, "additionalProperties": {"type": "number"}});
    assert_eq!(
        problems(typed, json!({"a": "x", "n": "y"})),
        ["data.n must be number (is string)"]
    );
    // A field a pattern of `patternProperties` names is not additional; one it does not, is.
    let patterned =
        json!({"patternProperties": {"^x": {"type": "integer"}}, "additionalProperties": false});
    assert!(problems(patterned.clone(), json!({"x1": 1})).is_empty());
    assert_eq!(
        problems(patterned.clone(), json!({"x1": "a"})),
        ["data.x1 must be integer (is string)"]
    );
    assert_eq!(
        problems(patterned, json!({"y": 1})),
        ["data has \"y\", which the schema does not allow"]
    );
}

#[test]
fn combinations_say_how_many_schemas_matched() {
    let any = json!({"anyOf": [{"type": "string"}, {"type": "number"}]});
    assert!(problems(any.clone(), json!(1)).is_empty());
    assert_eq!(
        problems(any, json!(null)),
        ["data matches none of the 2 schemas of anyOf"]
    );
    let one = json!({"oneOf": [{"type": "integer"}, {"minimum": 2}]});
    assert!(problems(one.clone(), json!(1)).is_empty());
    assert_eq!(
        problems(one.clone(), json!(3)),
        ["data must match exactly one schema of oneOf (matches more than one)"]
    );
    assert_eq!(
        problems(one, json!(1.5)),
        ["data must match exactly one schema of oneOf (matches none)"]
    );
    assert_eq!(
        problems(json!({"not": {"type": "null"}}), json!(null)),
        ["data must not match the schema of not"]
    );
    let all = json!({"allOf": [{"required": ["a"]}, {"required": ["b"]}]});
    assert_eq!(
        problems(all, json!({})),
        ["data is missing \"a\"", "data is missing \"b\""]
    );
    let conditional = json!({"if": {"properties": {"kind": {"const": "file"}}}, "then": {"required": ["path"]}, "else": {"required": ["url"]}});
    assert_eq!(
        problems(conditional.clone(), json!({"kind": "file"})),
        ["data is missing \"path\""]
    );
    assert_eq!(
        problems(conditional, json!({"kind": "link"})),
        ["data is missing \"url\""]
    );
    assert_eq!(problems(json!(false), json!(1)), ["data is not allowed"]);
}

#[test]
fn arrays_take_tuples_contains_and_both_spellings() {
    let tuple = json!({"prefixItems": [{"type": "string"}, {"type": "number"}], "items": false});
    assert!(problems(tuple.clone(), json!(["a", 1])).is_empty());
    assert_eq!(
        problems(tuple, json!(["a", 1, true])),
        ["data[2] is not allowed"]
    );
    // Draft 7's: `items` a list, `additionalItems` the rest.
    let draft7 = json!({"items": [{"type": "string"}], "additionalItems": {"type": "number"}});
    assert_eq!(
        problems(draft7, json!(["a", "b"])),
        ["data[1] must be number (is string)"]
    );
    let contains = json!({"contains": {"type": "number"}, "minContains": 2, "maxContains": 3});
    assert!(problems(contains.clone(), json!([1, "x", 2])).is_empty());
    assert_eq!(
        problems(contains.clone(), json!(["x", 1])),
        ["data must have at least 2 items matching contains (has 1)"]
    );
    assert_eq!(
        problems(contains, json!([1, 2, 3, 4])),
        ["data must have at most 3 items matching contains (has 4)"]
    );
}

#[test]
fn a_ref_points_into_the_schema_and_one_that_loops_ends() {
    let schema = json!({"$defs": {"node": {"type": "object", "required": ["name"], "properties": {
        "name": {"type": "string"}, "children": {"type": "array", "items": {"$ref": "#/$defs/node"}}
    }}}, "$ref": "#/$defs/node"});
    let tree = json!({"name": "a", "children": [{"name": "b", "children": [{}]}]});
    assert_eq!(
        problems(schema, tree),
        ["data.children[0].children[0] is missing \"name\""]
    );
    let looped = json!({"$ref": "#"});
    assert_eq!(
        problems(looped, json!(1)),
        ["data: the schema's $ref nests deeper than 32"]
    );
    // One outside the schema is not followed: the data is not held to it.
    assert!(problems(json!({"$ref": "https://example.com/x.json"}), json!(1)).is_empty());
}

#[test]
fn many_failures_are_cut_to_the_first_twenty() {
    let schema = json!({"type": "array", "items": {"type": "string"}});
    let data = Value::Array((0..50).map(|n| json!(n)).collect());
    assert_eq!(problems(schema, data).len(), 20);
}

#[test]
fn what_is_not_checked_here_is_named_once() {
    let schema = Schema::new(json!({"properties": {
        "pattern": {"type": "string", "pattern": "^(?!a)"},
        "b": {"format": "idn-email", "pattern": "x"},
        "c": {"$ref": "other.json"}
    }}));
    let mut unchecked = schema.unchecked();
    unchecked.sort_unstable();
    assert_eq!(unchecked, ["$ref outside the schema", "format", "pattern"]);
}

#[test]
fn a_ref_followed_into_deep_data_has_no_limit_but_one_that_only_loops_has() {
    let schema = json!({"$defs": {"n": {"type": "object", "properties": {"c": {"$ref": "#/$defs/n"}}, "required": ["v"]}}, "$ref": "#/$defs/n"});
    let mut deep = json!({"v": 0});
    for level in 1..100 {
        deep = json!({"v": level, "c": deep});
    }
    assert!(problems(schema.clone(), deep.clone()).is_empty());
    deep["c"]["c"]["c"] = json!({});
    assert_eq!(problems(schema, deep), ["data.c.c.c is missing \"v\""]);
    // Through a combination too: no step into the data, so the loop is cut.
    assert_eq!(
        problems(json!({"allOf": [{"$ref": "#"}]}), json!(1)),
        ["data: the schema's $ref nests deeper than 32"]
    );
}

#[test]
fn a_ref_named_many_times_is_compiled_once() {
    let one =
        json!({"$defs": {"n": {"type": "string"}}, "properties": {"a": {"$ref": "#/$defs/n"}}});
    let many = json!({"$defs": {"n": {"type": "string"}}, "properties": {
        "a": {"$ref": "#/$defs/n"}, "b": {"$ref": "#/$defs/n"}, "c": {"$ref": "#/$defs/n"}}});
    let nodes = |schema: Value| Schema::new(schema).nodes.len();
    // Each further field is its own node, and points at the one `n`.
    assert_eq!(nodes(many) - nodes(one), 2);
}

#[test]
fn required_fields_are_counted_where_properties_name_them_and_looked_up_where_not() {
    let named = json!({"required": ["a", "b"], "properties": {"a": {}, "b": {}, "c": {}}});
    assert!(problems(named.clone(), json!({"a": 1, "b": 2})).is_empty());
    assert_eq!(
        problems(named, json!({"a": 1, "c": 2})),
        ["data is missing \"b\""]
    );
    let unnamed = json!({"required": ["a", "z"], "properties": {"a": {}}});
    assert_eq!(
        problems(unnamed, json!({"a": 1})),
        ["data is missing \"z\""]
    );
    let names: Vec<String> = (0..70).map(|n| format!("f{n}")).collect();
    let mut data = serde_json::Map::new();
    let mut properties = serde_json::Map::new();
    for name in &names {
        properties.insert(name.clone(), json!({}));
        data.insert(name.clone(), json!(1));
    }
    let wide = json!({"required": names, "properties": properties});
    assert!(problems(wide.clone(), Value::Object(data.clone())).is_empty());
    data.remove("f69");
    assert_eq!(
        problems(wide, Value::Object(data)),
        ["data is missing \"f69\""]
    );
}

#[test]
fn each_draft_reads_its_own_keywords() {
    let draft4 = |schema: Value| {
        let mut schema = schema;
        schema["$schema"] = json!("http://json-schema.org/draft-04/schema#");
        schema
    };
    // Draft 4: `exclusiveMinimum` a flag on `minimum`; `1.0` no integer; `if` a word of its own.
    assert_eq!(
        problems(
            draft4(json!({"minimum": 5, "exclusiveMinimum": true})),
            json!(5)
        ),
        ["data must be > 5 (is 5)"]
    );
    assert!(problems(json!({"minimum": 5, "exclusiveMinimum": true}), json!(5)).is_empty());
    assert_eq!(
        problems(draft4(json!({"type": "integer"})), json!(1.0)),
        ["data must be integer (is number)"]
    );
    assert!(problems(draft4(json!({"if": true, "then": false})), json!(1)).is_empty());
    // Up to draft 7, a `$ref` hides what is beside it.
    let beside = json!({"$schema": "http://json-schema.org/draft-07/schema#",
        "definitions": {"s": {"type": "string"}}, "$ref": "#/definitions/s", "enum": ["a"]});
    assert!(problems(beside.clone(), json!("b")).is_empty());
    let mut later = beside;
    later["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
    later["$defs"] = later["definitions"].clone();
    assert_eq!(
        problems(later, json!("b")),
        ["data must be one of [\"a\"] (is \"b\")"]
    );
    // Draft 7's `dependencies`: a list of names, or a schema.
    let deps = json!({"$schema": "http://json-schema.org/draft-07/schema#",
        "dependencies": {"a": ["b"], "c": {"required": ["d"]}}});
    assert_eq!(
        problems(deps.clone(), json!({"a": 1})),
        ["data has \"a\", so it must have \"b\""]
    );
    assert_eq!(problems(deps, json!({"c": 1})), ["data is missing \"d\""]);
}

#[test]
fn a_ref_lands_on_an_anchor_an_id_or_a_pointer_inside_one() {
    let draft4 = json!({"$schema": "http://json-schema.org/draft-04/schema#",
        "definitions": {"jump": {"id": "#jump", "minimum": 0}}, "properties": {"d": {"$ref": "#jump"}}});
    assert_eq!(
        problems(draft4, json!({"d": -1})),
        ["data.d must be >= 0 (is -1)"]
    );
    let anchored = json!({"$defs": {"n": {"$anchor": "num", "type": "number"}}, "properties": {"a": {"$ref": "#num"}}});
    assert_eq!(
        problems(anchored, json!({"a": "x"})),
        ["data.a must be number (is string)"]
    );
    let ids = json!({"$id": "http://example.com/root.json", "$defs": {
        "item": {"$id": "item.json", "type": "object", "required": ["id"], "$defs": {"id": {"type": "integer"}},
            "properties": {"id": {"$ref": "#/$defs/id"}}}},
        "items": {"$ref": "item.json"}});
    assert!(problems(ids.clone(), json!([{"id": 1}])).is_empty());
    assert_eq!(
        problems(ids, json!([{"id": "x"}])),
        ["data[0].id must be integer (is string)"]
    );
    let remote = Schema::new(json!({"$ref": "https://example.com/elsewhere.json"}));
    assert_eq!(remote.unchecked(), ["$ref outside the schema"]);
}

#[test]
fn numbers_past_two_to_the_53rd_compare_exactly() {
    let schema = json!({"minimum": 9007199254740993_u64});
    assert_eq!(
        problems(schema.clone(), json!(9007199254740992_u64)),
        ["data must be >= 9007199254740993 (is 9007199254740992)"]
    );
    assert!(problems(schema, json!(9007199254740993_u64)).is_empty());
    assert_eq!(
        problems(json!({"maximum": 1.5}), json!(2)),
        ["data must be <= 1.5 (is 2)"]
    );
    assert!(problems(json!({"exclusiveMaximum": 2.5}), json!(2)).is_empty());
}

#[test]
fn unevaluated_fields_and_items_count_what_the_rest_looked_at() {
    let schema = json!({"properties": {"a": {}}, "allOf": [{"properties": {"b": {}}}],
        "anyOf": [{"properties": {"c": {"const": 1}}, "required": ["c"]}, {"required": ["zz"]}],
        "unevaluatedProperties": false});
    assert!(problems(schema.clone(), json!({"a": 1, "b": 2, "c": 1})).is_empty());
    assert_eq!(
        problems(schema, json!({"a": 1, "c": 1, "d": 4})),
        ["data has \"d\", which the schema does not allow"]
    );
    let items = json!({"prefixItems": [{"type": "string"}], "contains": {"type": "number"}, "unevaluatedItems": false});
    assert!(problems(items.clone(), json!(["a", 1, 2])).is_empty());
    assert_eq!(
        problems(items, json!(["a", 1, true])),
        ["data[2] is not allowed"]
    );
}

#[test]
fn many_enum_options_and_long_lists_are_looked_up_by_hash() {
    let options: Vec<Value> = (0..500).map(|n| json!(format!("o{n}"))).collect();
    let schema = json!({"enum": options});
    assert!(problems(schema.clone(), json!("o499")).is_empty());
    assert_eq!(problems(schema, json!("o500")).len(), 1);
    let mut long: Vec<Value> = (0..300).map(|n| json!(n)).collect();
    long.push(json!(150.0));
    assert_eq!(
        problems(json!({"uniqueItems": true}), Value::Array(long)),
        ["data must have unique items (data[150] and data[300] are equal)"]
    );
}

#[test]
fn a_value_of_no_kind_enum_lists_fails_and_one_far_too_long_too() {
    let options: Vec<Value> = (0..30).map(|n| json!(format!("o{n}"))).collect();
    let schema = json!({"enum": options});
    assert_eq!(problems(schema.clone(), json!({"o1": 1})).len(), 1);
    assert!(problems(schema, json!("o29")).is_empty());
    let long = "é".repeat(3000);
    assert_eq!(
        problems(json!({"maxLength": 512}), json!(long)),
        ["data must have at most 512 characters (has 3000)"]
    );
    assert!(!Schema::new(json!({"maxLength": 512})).matches(&json!("x".repeat(2049))));
    assert!(Schema::new(json!({"maxLength": 512})).matches(&json!("é".repeat(512))));
}

#[test]
fn an_object_with_many_properties_finds_its_fields_by_hash() {
    let properties: serde_json::Map<String, Value> = (0..30)
        .map(|n| (format!("p{n}"), json!({"type": "integer"})))
        .collect();
    let schema =
        json!({"properties": properties, "required": ["p3", "p29"], "additionalProperties": false});
    assert!(problems(schema.clone(), json!({"p3": 1, "p29": 2, "p10": 3})).is_empty());
    assert_eq!(
        problems(schema.clone(), json!({"p3": 1, "p29": "x"})),
        ["data.p29 must be integer (is string)"]
    );
    assert_eq!(
        problems(schema.clone(), json!({"p3": 1})),
        ["data is missing \"p29\""]
    );
    assert_eq!(
        problems(schema, json!({"p3": 1, "p29": 2, "zz": 0})),
        ["data has \"zz\", which the schema does not allow"]
    );
}

#[test]
fn formats_are_checked_and_one_not_checked_here_is_said() {
    let schema = json!({"properties": {"at": {"format": "date-time"}, "id": {"format": "uuid"}, "own": {"format": "my-own"}}});
    assert!(problems(schema.clone(), json!({"at": "2026-11-08T10:00:00Z", "id": "550e8400-e29b-41d4-a716-446655440000", "own": "x"})).is_empty());
    assert_eq!(
        problems(schema.clone(), json!({"at": "2026-11-08 10:00", "id": "x"})),
        [
            "data.at must be a valid date-time (is \"2026-11-08 10:00\")",
            "data.id must be a valid uuid (is \"x\")",
        ]
    );
    assert!(
        Schema::new(schema).unchecked().is_empty(),
        "a format of the schema's own asks nothing"
    );
    assert_eq!(
        Schema::new(json!({"format": "idn-email"})).unchecked(),
        ["format"]
    );
}

#[test]
fn a_ref_lands_in_another_document_given_with_its_uri() {
    let documents = [(
        "https://example.com/defs.json".to_string(),
        json!({"$defs": {"id": {"type": "integer", "minimum": 1}}}),
    )];
    let schema = Schema::with_documents(
        json!({"properties": {"id": {"$ref": "https://example.com/defs.json#/$defs/id"}}}),
        &documents,
    );
    assert!(schema.unchecked().is_empty());
    assert!(schema.matches(&json!({"id": 3})));
    assert_eq!(
        schema.mismatches(&json!({"id": 0})),
        ["data.id must be >= 1 (is 0)"]
    );
}

#[test]
fn a_metaschema_without_the_validation_vocabulary_asks_none_of_its_keywords() {
    let meta = (
        "https://example.com/meta/no-validation".to_string(),
        json!({"$schema": "https://json-schema.org/draft/2020-12/schema",
            "$vocabulary": {"https://json-schema.org/draft/2020-12/vocab/applicator": true,
                "https://json-schema.org/draft/2020-12/vocab/core": true}}),
    );
    let schema = Schema::with_documents(
        json!({"$schema": "https://example.com/meta/no-validation",
            "properties": {"n": {"minimum": 10}}, "required": ["x"], "type": "string"}),
        &[meta],
    );
    assert!(schema.matches(&json!({"n": 1})));
    let applied = Schema::with_documents(
        json!({"$schema": "https://example.com/meta/no-validation", "properties": {"n": false}}),
        &[(
            "https://example.com/meta/no-validation".to_string(),
            json!({"$vocabulary": {"https://json-schema.org/draft/2020-12/vocab/core": true}}),
        )],
    );
    assert!(
        !applied.matches(&json!({"n": 1})),
        "applicators still apply"
    );
}

#[test]
fn patterns_are_checked_where_a_string_and_a_field_name_are() {
    let schema = json!({"properties": {"code": {"pattern": "^[A-Z]{3}$"}},
        "propertyNames": {"pattern": "^[a-z]+$"}});
    assert!(problems(schema.clone(), json!({"code": "USD"})).is_empty());
    assert_eq!(
        problems(schema.clone(), json!({"code": "usd"})),
        ["data.code must match the pattern \"^[A-Z]{3}$\" (is \"usd\")"]
    );
    assert_eq!(
        problems(schema, json!({"Code": 1})),
        ["data.Code must match the pattern \"^[a-z]+$\" (is \"Code\")"]
    );
}

#[test]
fn a_leaf_that_asks_only_a_type_is_judged_where_it_stands_and_still_said() {
    let schema = json!({"$defs": {"time": {"type": "string", "description": "a time"},
            "count": {"$ref": "#/$defs/whole"}, "whole": {"type": "integer"}},
        "properties": {"at": {"$ref": "#/$defs/time"}, "n": {"$ref": "#/$defs/count"},
            "price": {"type": "number"}, "any": true},
        "items": {"type": ["string", "null"]}});
    assert!(
        problems(
            schema.clone(),
            json!({"at": "noon", "n": 2.0, "price": 1, "any": [1]})
        )
        .is_empty()
    );
    assert_eq!(
        problems(schema.clone(), json!({"at": 1})),
        ["data.at must be string (is integer)"]
    );
    assert_eq!(
        problems(schema.clone(), json!({"n": 2.5})),
        ["data.n must be integer (is number)"]
    );
    assert_eq!(
        problems(schema, json!(["a", null, 3])),
        ["data[2] must be string or null (is integer)"]
    );
}

#[test]
fn a_union_of_types_alone_is_judged_by_its_types_and_still_said_whole() {
    let schema = json!({"$defs": {"text": {"type": "string"}},
    "properties": {
        "any": {"items": {"anyOf": [{"$ref": "#/$defs/text"}, {"type": "number"}]}},
        "one": {"items": {"oneOf": [{"type": "string"}, {"type": "integer"}, {"type": "null"}]}},
        "both": {"items": {"oneOf": [{"type": "integer"}, {"type": "number"}]}},
        "nested": {"anyOf": [{"anyOf": [{"type": "null"}, {"type": "boolean"}]}, {"type": "array"}]},
        "typed": {"type": "integer", "anyOf": [{"type": "number"}, {"type": "string"}]}
    }});
    let fine = json!({"any": ["a", 1, 2.5], "one": ["a", 3, null], "both": [2.5],
        "nested": [true], "typed": 4});
    assert!(problems(schema.clone(), fine).is_empty());
    assert!(!problems(schema.clone(), json!({"any": [null]})).is_empty());
    assert!(!problems(schema.clone(), json!({"one": [2.5]})).is_empty());
    // 3 is an integer and a number: two branches of a `oneOf` match it.
    assert!(!problems(schema.clone(), json!({"both": [3]})).is_empty());
    assert!(!problems(schema.clone(), json!({"nested": "x"})).is_empty());
    assert!(!problems(schema.clone(), json!({"typed": "x"})).is_empty());
    assert!(!problems(schema, json!({"typed": 4.5})).is_empty());
}

#[test]
fn property_names_false_refuses_every_field_and_says_where() {
    let schema = json!({"propertyNames": false});
    assert!(problems(schema.clone(), json!({})).is_empty());
    assert_eq!(
        problems(schema.clone(), json!({"a": 1})),
        ["data.a is not allowed"]
    );
    assert!(problems(json!({"not": schema}), json!({"a": 1})).is_empty());
}
