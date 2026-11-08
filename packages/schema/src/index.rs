//! Where a `$ref` can land in one document: each resource by its URI (`$id`, or `id` in draft 4,
//! resolved against the one around it), each anchor (`$anchor`, `$dynamicAnchor`, or before
//! 2019-09 an id that is only a fragment), and then a JSON pointer inside. Only subschemas are
//! walked: an `$id` inside data (`enum`, `default`) or an unknown keyword names nothing.

use std::collections::HashMap;

use serde_json::Value;

use crate::draft::Draft;
use crate::uri::{self, BASE};

/// Keywords whose value is a schema.
const ONE: [&str; 13] = [
    "additionalProperties",
    "additionalItems",
    "items",
    "contains",
    "propertyNames",
    "not",
    "if",
    "then",
    "else",
    "unevaluatedProperties",
    "unevaluatedItems",
    "contentSchema",
    "$recursiveRef",
];
/// Keywords whose value is a list of schemas.
const LIST: [&str; 5] = ["allOf", "anyOf", "oneOf", "prefixItems", "items"];
/// Keywords whose value maps names to schemas.
const MAP: [&str; 6] = [
    "properties",
    "patternProperties",
    "$defs",
    "definitions",
    "dependentSchemas",
    "dependencies",
];

/// A document's resources and anchors.
pub(crate) struct Index<'j> {
    resources: HashMap<String, &'j Value>,
    anchors: HashMap<String, &'j Value>,
    /// How many resources give each `$dynamicAnchor` (2020-12), and how many say
    /// `$recursiveAnchor: true` (2019-09): with one, a dynamic reference lands where a plain one
    /// does.
    pub(crate) dynamic: HashMap<String, usize>,
    pub(crate) recursive: usize,
}

impl<'j> Index<'j> {
    /// The index of `root` and of the other `documents` it may refer to, each by its URI.
    pub(crate) fn build(root: &'j Value, documents: &'j [(String, Value)], draft: Draft) -> Self {
        let mut index = Index {
            resources: HashMap::new(),
            anchors: HashMap::new(),
            dynamic: HashMap::new(),
            recursive: 0,
        };
        for (uri, document) in documents {
            let named = uri::split(uri).0;
            index.resources.insert(named.to_string(), document);
            let own = match document.get("$schema") {
                Some(_) => Draft::of(document),
                None => draft,
            };
            index.walk(document, named, own);
        }
        index.resources.insert(BASE.to_string(), root);
        index.walk(root, BASE, draft);
        index
    }

    fn walk(&mut self, schema: &'j Value, base: &str, draft: Draft) {
        let Value::Object(rules) = schema else {
            return;
        };
        let hidden = draft.ref_alone() && rules.contains_key("$ref");
        let id = (rules.get(draft.id_keyword()).and_then(Value::as_str)).filter(|_| !hidden);
        let mut here = base.to_string();
        if let Some(id) = id {
            let resolved = uri::resolve(base, id);
            let (resource, fragment) = uri::split(&resolved);
            match (fragment.is_empty(), draft >= Draft::Nineteen) {
                (true, _) => {
                    self.resources.insert(resource.to_string(), schema);
                    here = resource.to_string();
                }
                // Before 2019-09 an id that is only a fragment is an anchor.
                (false, false) if !fragment.starts_with('/') => {
                    self.anchors.insert(resolved.clone(), schema);
                }
                (false, true | false) => {}
            }
        }
        let named = |key: &str| rules.get(key).and_then(Value::as_str);
        if let Some(name) = named("$anchor").filter(|_| draft >= Draft::Nineteen) {
            self.anchors.insert(format!("{here}#{name}"), schema);
        }
        if let Some(name) = named("$dynamicAnchor").filter(|_| draft == Draft::Twenty) {
            self.anchors.insert(format!("{here}#{name}"), schema);
            *self.dynamic.entry(name.to_string()).or_default() += 1;
        }
        if draft == Draft::Nineteen && rules.get("$recursiveAnchor") == Some(&Value::Bool(true)) {
            self.recursive += 1;
        }
        for (key, value) in rules {
            match value {
                Value::Array(items) if LIST.contains(&key.as_str()) => {
                    items.iter().for_each(|item| self.walk(item, &here, draft));
                }
                Value::Object(named) if MAP.contains(&key.as_str()) => {
                    named
                        .values()
                        .for_each(|item| self.walk(item, &here, draft));
                }
                _ if ONE.contains(&key.as_str()) => self.walk(value, &here, draft),
                Value::Null
                | Value::Bool(_)
                | Value::Number(_)
                | Value::String(_)
                | Value::Array(_)
                | Value::Object(_) => {}
            }
        }
    }

    /// Whether the metaschema `root` names, where it is one of the documents, takes the
    /// validation vocabulary: one that lists its `$vocabulary` without it asks for none of
    /// `type`, `minimum`, `required` and the like.
    pub(crate) fn validates(&self, root: &Value) -> bool {
        let named = root.get("$schema").and_then(Value::as_str).unwrap_or("");
        let meta = self.resources.get(uri::split(named).0);
        let listed = meta
            .and_then(|meta| meta.get("$vocabulary"))
            .and_then(Value::as_object);
        listed.is_none_or(|listed| listed.keys().any(|key| key.ends_with("/vocab/validation")))
    }

    /// What the absolute reference `target` lands on, with the base it is read with.
    pub(crate) fn find(&self, target: &str) -> Option<(&'j Value, String)> {
        let (resource, fragment) = uri::split(target);
        if !fragment.is_empty() && !fragment.starts_with('/') {
            return Some((*self.anchors.get(target)?, resource.to_string()));
        }
        let mut here = *self.resources.get(resource)?;
        for part in fragment.split('/').skip(1) {
            let part = decode(part)?;
            here = match here {
                Value::Object(map) => map.get(&part)?,
                Value::Array(list) => list.get(part.parse::<usize>().ok()?)?,
                Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => return None,
            };
        }
        Some((here, resource.to_string()))
    }
}

/// A pointer's segment as a name: `%XX` decoded, then `~1` and `~0`.
fn decode(part: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(part.len());
    let mut rest = part.bytes();
    while let Some(byte) = rest.next() {
        match byte {
            b'%' => {
                let high = char::from(rest.next()?).to_digit(16)?;
                let low = char::from(rest.next()?).to_digit(16)?;
                bytes.push(u8::try_from(high * 16 + low).ok()?);
            }
            other => bytes.push(other),
        }
    }
    let text = String::from_utf8(bytes).ok()?;
    Some(text.replace("~1", "/").replace("~0", "~"))
}

#[cfg(test)]
#[path = "index_tests.rs"]
mod tests;
