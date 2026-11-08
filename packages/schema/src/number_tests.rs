use std::cmp::Ordering;

use serde_json::{Number, Value, json};

use super::*;

#[test]
fn a_whole_float_is_read_as_its_integer_exactly() {
    assert_eq!(exact(0.0), Some(0));
    assert_eq!(exact(-0.0), Some(0));
    assert_eq!(exact(3.0), Some(3));
    assert_eq!(exact(-1024.0), Some(-1024));
    assert_eq!(exact(9_007_199_254_740_992.0), Some(9_007_199_254_740_992));
    assert_eq!(
        exact(1.0e30),
        Some(1_000_000_000_000_000_019_884_624_838_656)
    );
    assert_eq!(exact(-(2f64.powi(126))), Some(-(1_i128 << 126)));
    assert_eq!(exact(1.5), None);
    assert_eq!(exact(f64::INFINITY), None);
    assert_eq!(exact(f64::NAN), None);
    assert_eq!(exact(1.0e39), None, "past an i128");
}

#[test]
fn an_integer_and_a_float_compare_without_rounding() {
    let n = |value: Value| -> Number { serde_json::from_value(value).unwrap() };
    let cmp = |a, b| compare(&n(a), &n(b));
    assert_eq!(
        cmp(
            json!(9_007_199_254_740_993_u64),
            json!(9_007_199_254_740_992.0)
        ),
        Some(Ordering::Greater)
    );
    assert_eq!(cmp(json!(2), json!(2.5)), Some(Ordering::Less));
    assert_eq!(cmp(json!(3), json!(2.5)), Some(Ordering::Greater));
    assert_eq!(cmp(json!(-3), json!(-2.5)), Some(Ordering::Less));
    assert_eq!(cmp(json!(2), json!(2.0)), Some(Ordering::Equal));
    assert_eq!(cmp(json!(u64::MAX), json!(1.0e300)), Some(Ordering::Less));
    assert_eq!(
        cmp(json!(i64::MIN), json!(-1.0e300)),
        Some(Ordering::Greater)
    );
    assert_eq!(cmp(json!(1.5), json!(2.5)), Some(Ordering::Less));
}
