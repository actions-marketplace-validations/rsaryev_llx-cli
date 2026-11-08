use super::*;
use crate::fakes::Shaped;

#[test]
fn a_required_field_is_required_where_the_schema_lists_no_properties() {
    // JSON Schema's `required` stands on its own: an object without the field breaks it.
    let schema = Shaped(json!({"type": "object", "required": ["price"]}));
    assert_eq!(
        take(&json!({"data": {}}), &schema),
        Err("data is missing \"price\"".into())
    );
    assert_eq!(
        take(&json!({"data": {"price": 93}}), &schema),
        Ok(json!({"price": 93}))
    );
}

#[test]
fn a_result_call_is_checked_against_its_schema() {
    let schema = Shaped(
        json!({"type": "object", "required": ["price"], "properties": {"price": {"type": "number"}}}),
    );
    let call = json!({"data": {"price": 93}});
    assert_eq!(take(&call, &schema), Ok(json!({"price": 93})));
    let wrong = json!({"data": {"price": "93"}});
    assert_eq!(
        take(&wrong, &schema),
        Err("data.price must be number".into())
    );
}

#[test]
fn fields_put_beside_data_instead_of_inside_it_are_the_data() {
    // The schema has a `summary` of its own, as a pull request's description does: the model
    // wrote its fields at the top of the call, with no `data`.
    let schema = Shaped(
        json!({"type": "object", "required": ["summary", "risk"], "properties": {
            "summary": {"type": "string"}, "risk": {"enum": ["low", "medium", "high"]}
        }}),
    );
    let flat = json!({"summary": "Adds a cache.", "risk": "low"});
    assert_eq!(
        take(&flat, &schema),
        Ok(json!({"summary": "Adds a cache.", "risk": "low"}))
    );
}

#[test]
fn a_call_with_no_data_says_where_the_fields_go() {
    let schema = Shaped(
        json!({"type": "object", "required": ["price"], "properties": {"price": {"type": "number"}}}),
    );
    let call = json!({"summary": "€93"});
    let error = take(&call, &schema).unwrap_err();
    assert!(error.contains("inside `data`"), "{error}");
}
