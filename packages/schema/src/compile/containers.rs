//! The keywords of arrays and objects compiled, as the draft spells them: `items` a list with
//! `additionalItems` before 2020-12 (and taken so in 2020-12 too, where an LLM's schema often
//! writes it), `prefixItems` in 2020-12; `dependencies` before 2019-09 split by what it holds.

use serde_json::{Map, Value};

use super::Compiler;
use super::keywords::{count, names};
use crate::node::{Arrays, Extra, Id, Names, Objects};

impl<'j> Compiler<'j> {
    pub(super) fn arrays(
        &mut self,
        rules: &'j Map<String, Value>,
        base: &str,
    ) -> Option<Box<Arrays>> {
        let draft = self.draft;
        let get = |key: &str| rules.get(key).filter(|_| draft.knows(key));
        const KEYS: [&str; 9] = [
            "items",
            "prefixItems",
            "additionalItems",
            "minItems",
            "maxItems",
            "uniqueItems",
            "contains",
            "minContains",
            "maxContains",
        ];
        if !KEYS.iter().any(|key| get(key).is_some()) {
            return None;
        }
        let (prefix, rest) = match (get("prefixItems"), get("items")) {
            (Some(prefix @ Value::Array(_)), rest) => (Some(prefix), rest),
            (None | Some(_), Some(prefix @ Value::Array(_))) => {
                (Some(prefix), rules.get("additionalItems"))
            }
            (None | Some(_), rest) => (None, rest),
        };
        let contains = get("contains").map(|schema| {
            let least = count(rules, "minContains").filter(|_| draft.knows("minContains"));
            let most = count(rules, "maxContains").filter(|_| draft.knows("maxContains"));
            (self.node(schema, base), least.unwrap_or(1), most)
        });
        Some(Box::new(Arrays {
            prefix: self.many(prefix, base),
            rest: rest.map(|schema| self.node(schema, base)),
            min_items: count(rules, "minItems"),
            max_items: count(rules, "maxItems"),
            unique: get("uniqueItems") == Some(&Value::Bool(true)),
            contains,
        }))
    }

    pub(super) fn objects(
        &mut self,
        rules: &'j Map<String, Value>,
        base: &str,
    ) -> Option<Box<Objects>> {
        let draft = self.draft;
        let get = |key: &str| rules.get(key).filter(|_| draft.knows(key));
        const KEYS: [&str; 11] = [
            "required",
            "properties",
            "patternProperties",
            "additionalProperties",
            "propertyNames",
            "minProperties",
            "maxProperties",
            "dependentRequired",
            "dependentSchemas",
            "dependencies",
            "unevaluatedProperties",
        ];
        if !KEYS.iter().any(|key| get(key).is_some()) {
            return None;
        }
        let required: Names = names(get("required"));
        let named = get("properties").and_then(Value::as_object);
        let mut properties: Vec<(Box<str>, Id, u64)> = vec![];
        for (name, schema) in named.into_iter().flatten() {
            let place = required.iter().position(|field| **field == **name);
            let bit = place
                .filter(|place| *place < 64)
                .map_or(0, |place| 1 << place);
            properties.push((name.as_str().into(), self.node(schema, base), bit));
        }
        // In order, as a data object's fields come: a walk meets both in one pass.
        properties.sort_unstable_by(|(a, _, _), (b, _, _)| a.cmp(b));
        let properties = properties.into_boxed_slice();
        let bits = properties.iter().fold(0, |all, (_, _, bit)| all | bit);
        let all_named = required.len() <= 64 && bits.count_ones() as usize == required.len();
        let mut patterns = vec![];
        for (source, schema) in get("patternProperties")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            let place = self.pattern(source);
            patterns.push((place, self.node(schema, base)));
        }
        let extra = match get("additionalProperties") {
            None => Extra::Absent,
            Some(Value::Bool(true)) => Extra::Allowed,
            Some(Value::Bool(false)) => Extra::Refused,
            Some(schema) => Extra::Checked(self.node(schema, base)),
        };
        let (mut dependent, mut dependent_schemas) = (vec![], vec![]);
        let deps = [
            get("dependentRequired"),
            get("dependentSchemas"),
            get("dependencies"),
        ];
        for (name, need) in deps
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .flatten()
        {
            match need {
                Value::Array(_) => dependent.push((name.as_str().into(), names(Some(need)))),
                Value::Object(_) | Value::Bool(_) => {
                    dependent_schemas.push((name.as_str().into(), self.node(need, base)));
                }
                Value::Null | Value::Number(_) | Value::String(_) => {}
            }
        }
        let min_fields = count(rules, "minProperties");
        let max_fields = count(rules, "maxProperties");
        let closing = min_fields.is_some()
            || max_fields.is_some()
            || !dependent.is_empty()
            || !dependent_schemas.is_empty();
        Some(Box::new(Objects {
            closing,
            lookup: crate::hash::Names::of(&properties),
            required,
            properties,
            required_bits: all_named.then_some(bits),
            patterns: patterns.into_boxed_slice(),
            extra,
            names: get("propertyNames").map(|schema| self.node(schema, base)),
            min_fields,
            max_fields,
            dependent: dependent.into_boxed_slice(),
            dependent_schemas: dependent_schemas.into_boxed_slice(),
        }))
    }
}
