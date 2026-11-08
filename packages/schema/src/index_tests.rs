use serde_json::json;

use super::*;

#[test]
fn ids_anchors_and_pointers_are_found_from_their_bases() {
    let root = json!({
        "$id": "http://example.com/root.json",
        "$defs": {
            "a": {"$anchor": "here", "type": "integer"},
            "b": {"$id": "item.json", "$defs": {"c": {"type": "string"}}},
            "odd%/~": {"const": 1}
        },
        "enum": [{"$id": "http://example.com/data.json"}]
    });
    let index = Index::build(&root, &[], Draft::Twenty);
    let found = |target: &str| {
        index
            .find(target)
            .map(|(value, base)| (value.clone(), base))
    };
    let (anchored, base) = found("http://example.com/root.json#here").unwrap();
    assert_eq!(
        (anchored["type"].as_str(), base.as_str()),
        (Some("integer"), "http://example.com/root.json")
    );
    let (nested, base) = found("http://example.com/item.json#/$defs/c").unwrap();
    assert_eq!(
        (nested["type"].as_str(), base.as_str()),
        (Some("string"), "http://example.com/item.json")
    );
    assert_eq!(
        found("http://example.com/root.json#/$defs/odd%25~1~0")
            .unwrap()
            .0["const"],
        1
    );
    assert!(
        found("http://example.com/data.json").is_none(),
        "an $id inside data names nothing"
    );
}

#[test]
fn before_2019_09_an_id_that_is_a_fragment_is_an_anchor_and_a_ref_hides_its_id() {
    let draft4 = json!({"definitions": {"x": {"id": "#jump", "type": "number"}}});
    assert!(
        Index::build(&draft4, &[], Draft::Four)
            .find("json-schema:///#jump")
            .is_some()
    );
    let draft7 = json!({"definitions": {"x": {"$id": "other.json", "$ref": "#/definitions/y"}}});
    assert!(
        Index::build(&draft7, &[], Draft::Seven)
            .find("json-schema:///other.json")
            .is_none()
    );
}
