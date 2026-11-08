//! Keywords read into a node's plain values: `type`'s bits, the number bounds as the draft says
//! them, counts and lists of names.

use serde_json::{Map, Number, Value};

use crate::draft::Draft;
use crate::node::{ARRAY, BOOLEAN, INTEGER, NULL, NUMBER, Names, OBJECT, STRING, Sign};
use crate::number::exact;

/// `type`: its bits (every kind for a name it does not know, none for one that is not a name),
/// and its names.
pub(super) fn types(kind: Option<&Value>) -> (Option<u8>, Names) {
    let listed = match kind {
        None => return (None, Box::default()),
        Some(Value::Array(many)) => many.as_slice(),
        Some(one) => std::slice::from_ref(one),
    };
    let names: Names = listed
        .iter()
        .filter_map(Value::as_str)
        .map(Into::into)
        .collect();
    let bits = names.iter().fold(0, |all, name| {
        all | match &**name {
            "null" => NULL,
            "boolean" => BOOLEAN,
            "integer" => INTEGER,
            "number" => NUMBER | INTEGER,
            "string" => STRING,
            "array" => ARRAY,
            "object" => OBJECT,
            _ => u8::MAX,
        }
    });
    (Some(bits), names)
}

/// The number bounds, in the order a message says them: `minimum`, `exclusiveMinimum`, `maximum`,
/// `exclusiveMaximum`; in draft 4, `exclusiveMinimum: true` makes `minimum` strict instead.
pub(super) fn limits(rules: &Map<String, Value>, draft: Draft) -> Box<[(Number, Sign)]> {
    let number = |key: &str| match rules.get(key) {
        Some(Value::Number(n)) => Some(n.clone()),
        Some(_) | None => None,
    };
    let flag = |key: &str| draft.flag_bounds() && rules.get(key) == Some(&Value::Bool(true));
    let low = match flag("exclusiveMinimum") {
        true => Sign::Above,
        false => Sign::AtLeast,
    };
    let high = match flag("exclusiveMaximum") {
        true => Sign::Below,
        false => Sign::AtMost,
    };
    let exclusive = |key: &str| number(key).filter(|_| !draft.flag_bounds());
    [
        number("minimum").map(|m| (m, low)),
        exclusive("exclusiveMinimum").map(|m| (m, Sign::Above)),
        number("maximum").map(|m| (m, high)),
        exclusive("exclusiveMaximum").map(|m| (m, Sign::Below)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

pub(super) fn count(rules: &Map<String, Value>, key: &str) -> Option<u64> {
    match rules.get(key)? {
        // A count written `2.0` is the count 2.
        Value::Number(n) => n.as_u64().or_else(|| {
            let whole = exact(n.as_f64()?)?;
            // A count past u64 is one no data reaches: the most there is.
            Some(u64::try_from(whole.max(0)).unwrap_or(u64::MAX))
        }),
        Value::Null | Value::Bool(_) | Value::String(_) | Value::Array(_) | Value::Object(_) => {
            None
        }
    }
}

pub(super) fn names(list: Option<&Value>) -> Names {
    let list = list
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    list.iter()
        .filter_map(Value::as_str)
        .map(Into::into)
        .collect()
}
