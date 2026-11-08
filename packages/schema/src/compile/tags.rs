//! Tagged unions: a `oneOf` or `anyOf` whose branches each require a field of one name and hold
//! it to a few strings of their own (`"type": {"const": "cargo"}`), as the schemas of tool calls
//! and of an LLM's structured answers are written. In an object, that field's value names the
//! only branches that can match: a branch that requires it and does not allow the value fails
//! whatever else it asks, as its keywords all must hold. The walk then tries those alone.

use serde_json::Value;

use crate::node::{Id, Node, Rules};

/// A value of the tag with the places of the branches that allow it.
type Leaves = (Box<str>, Box<[u32]>);

/// A union's tag: the field, and which branches each of its values leaves to try.
pub(crate) struct Tag {
    name: Box<str>,
    /// Each value, in order, with the places of the branches that allow it.
    by_value: Box<[Leaves]>,
    /// The branches the field does not settle: tried whatever it holds.
    untagged: Box<[u32]>,
}

/// How deep a branch's `$ref`s and `allOf`s are followed to find its tag.
const DEPTH: usize = 8;

impl Tag {
    /// The branches of the union `value` can match, in two lists; `None` where the tag says
    /// nothing (the value is not an object), and every branch is tried.
    pub(crate) fn picks(&self, value: &Value) -> Option<(&[u32], &[u32])> {
        let fields = value.as_object()?;
        let tagged: &[u32] = match fields.get(&*self.name) {
            Some(Value::String(text)) => (self.by_value)
                .binary_search_by(|(option, _)| (**option).cmp(text.as_str()))
                .ok()
                .and_then(|at| self.by_value.get(at))
                .map_or(&[], |(_, branches)| branches),
            // Each tagged branch requires the field and allows only strings.
            _ => &[],
        };
        Some((tagged, &self.untagged))
    }
}

/// Every union of `nodes` given its tag, where its branches have one.
pub(crate) fn mark(nodes: &mut [Node]) {
    let found: Vec<(usize, Option<Tag>, Option<Tag>)> = (nodes.iter().enumerate())
        .filter_map(|(at, node)| match node {
            Node::Rules(rules) => {
                let any = tag(nodes, &rules.any_of);
                let one = tag(nodes, &rules.one_of);
                (any.is_some() || one.is_some()).then_some((at, any, one))
            }
            Node::Any | Node::Never => None,
        })
        .collect();
    for (at, any, one) in found {
        if let Some(Node::Rules(rules)) = nodes.get_mut(at) {
            rules.any_tag = any.map(Box::new);
            rules.one_tag = one.map(Box::new);
        }
    }
}

/// The tag of a union of `branches`, where one field settles all but at most one of them.
fn tag(nodes: &[Node], branches: &[Id]) -> Option<Tag> {
    let first = *branches.first()?;
    if branches.len() < 3 {
        return None;
    }
    let mut names = vec![];
    fields(nodes, first, DEPTH, &mut names);
    names.sort_unstable();
    names.dedup();
    names
        .into_iter()
        .filter_map(|name| tagged_by(nodes, branches, name))
        .min_by_key(|tag| tag.untagged.len())
        .filter(|tag| tag.untagged.len() + 1 < branches.len())
}

/// The union's tag by the field `name`.
fn tagged_by(nodes: &[Node], branches: &[Id], name: &str) -> Option<Tag> {
    let mut by_value: Vec<(Box<str>, Vec<u32>)> = vec![];
    let mut untagged = vec![];
    for (place, &branch) in branches.iter().enumerate() {
        let place = u32::try_from(place).ok()?;
        let values = required(nodes, branch, name, DEPTH)
            .then(|| strings(nodes, branch, name, DEPTH))
            .flatten();
        let Some(values) = values else {
            untagged.push(place);
            continue;
        };
        for value in values {
            match by_value.iter_mut().find(|(known, _)| **known == *value) {
                Some((_, places)) => places.push(place),
                None => by_value.push((value.into(), vec![place])),
            }
        }
    }
    by_value.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    Some(Tag {
        name: name.into(),
        by_value: (by_value.into_iter())
            .map(|(value, places)| (value, places.into()))
            .collect(),
        untagged: untagged.into(),
    })
}

/// The rules a branch holds a value to: its own, those of its `$ref` and of its `allOf`.
fn chain<'n>(
    nodes: &'n [Node],
    id: Id,
    depth: usize,
    each: &mut dyn FnMut(&'n Rules) -> bool,
) -> bool {
    let Some(Node::Rules(rules)) = nodes.get(id) else {
        return false;
    };
    if each(rules) {
        return true;
    }
    let Some(depth) = depth.checked_sub(1) else {
        return false;
    };
    let mut next = rules
        .reference
        .into_iter()
        .chain(rules.all_of.iter().copied());
    next.any(|id| chain(nodes, id, depth, each))
}

/// The names of the fields the branch both requires and names in `properties`.
fn fields<'n>(nodes: &'n [Node], id: Id, depth: usize, out: &mut Vec<&'n str>) {
    let Some(Node::Rules(rules)) = nodes.get(id) else {
        return;
    };
    if let Some(objects) = &rules.object {
        out.extend(objects.required.iter().map(|name| &**name));
    }
    if let Some(depth) = depth.checked_sub(1) {
        for next in rules
            .reference
            .into_iter()
            .chain(rules.all_of.iter().copied())
        {
            fields(nodes, next, depth, out);
        }
    }
}

/// Whether the branch requires the field `name`.
fn required(nodes: &[Node], id: Id, name: &str, depth: usize) -> bool {
    chain(nodes, id, depth, &mut |rules| {
        (rules.object.as_ref()).is_some_and(|objects| objects.required.iter().any(|n| **n == *name))
    })
}

/// The strings the branch holds the field `name` to, where its schema there lists only strings.
fn strings<'n>(nodes: &'n [Node], id: Id, name: &str, depth: usize) -> Option<Vec<&'n str>> {
    let mut found = None;
    chain(nodes, id, depth, &mut |rules| {
        let field = (rules.object.as_ref())
            .and_then(|objects| objects.properties.iter().find(|(n, _, _)| **n == *name));
        if let Some((_, field, _)) = field {
            found = allowed(nodes, *field, depth);
        }
        found.is_some()
    });
    found
}

/// The strings a schema allows, where it allows only a list of them (`const` or `enum`).
fn allowed(nodes: &[Node], id: Id, depth: usize) -> Option<Vec<&str>> {
    let mut found = None;
    chain(nodes, id, depth, &mut |rules| {
        found = match (&rules.constant, &rules.options) {
            (Some(Value::String(one)), _) => Some(vec![one.as_str()]),
            (_, Some(Value::Array(options))) => options.iter().map(Value::as_str).collect(),
            _ => None,
        };
        found.is_some()
    });
    found
}

#[cfg(test)]
#[path = "tags_tests.rs"]
mod tests;
