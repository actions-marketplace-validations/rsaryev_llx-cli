use serde_json::json;

use super::*;

#[test]
fn equal_json_hashes_alike() {
    assert_eq!(hash(&json!(1)), hash(&json!(1.0)));
    assert_eq!(
        hash(&json!({"a": [1, "x"], "b": null})),
        hash(&json!({"b": null, "a": [1.0, "x"]}))
    );
    assert_ne!(hash(&json!("a")), hash(&json!(["a"])));
    assert_ne!(hash(&json!(1.5)), hash(&json!(1)));
}

#[test]
fn a_long_list_finds_its_first_repeat_by_hash_as_a_short_one_does() {
    let mut items: Vec<Value> = (0..100).map(|n| json!(format!("item-{n}"))).collect();
    items.push(json!("item-40"));
    items.push(json!("item-7"));
    assert_eq!(
        duplicate(&items),
        Some((40, 100)),
        "the first item that repeats"
    );
    assert_eq!(duplicate(&items[..100]), None);
    let numbers: Vec<Value> = (0..40).map(|n| json!(n)).chain([json!(3.0)]).collect();
    assert_eq!(duplicate(&numbers), Some((3, 40)));
}

#[test]
fn many_options_are_a_table_and_few_are_not() {
    let options: Vec<Value> = (0..1000)
        .map(|n| json!(format!("opt-{n}")))
        .chain([json!(2), json!({"a": 1})])
        .collect();
    let table = Table::of(&options).unwrap();
    assert!(table.holds(&options, &json!("opt-999")));
    assert!(table.holds(&options, &json!(2.0)));
    assert!(table.holds(&options, &json!({"a": 1.0})));
    assert!(!table.holds(&options, &json!("opt-1000")));
    assert!(Table::of(&options[..3]).is_none());
}

#[test]
fn many_names_are_found_by_hash_and_few_are_not_tabled() {
    let names: Vec<(Box<str>, usize, u64)> = (0..40)
        .map(|n| (format!("field_{n}").into(), n, 0))
        .collect();
    let table = Names::of(&names).unwrap();
    assert_eq!(table.find(&names, "field_39"), Some(39));
    assert_eq!(table.find(&names, "field_0"), Some(0));
    assert_eq!(table.find(&names, "field_40"), None);
    assert_eq!(table.find(&names, ""), None);
    assert!(Names::of(&names[..Names::PAST]).is_none());
}

#[test]
fn a_short_list_names_the_first_repeat_too() {
    let items = [json!("a"), json!("b"), json!("b"), json!("a")];
    assert_eq!(duplicate(&items), Some((1, 2)));
}
