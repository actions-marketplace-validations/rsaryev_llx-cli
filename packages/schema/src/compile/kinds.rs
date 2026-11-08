//! Types alone, settled once the nodes are built: a `$ref` and nothing else asks the types of
//! what it lands on. Unions of types: an `anyOf` whose branches each ask only for types (`{"type": "string"}`, a
//! `$ref` to one, `true`) asks only for the union of them, and a `oneOf` of such branches too,
//! where no two share a type (a value then matches one of them at most: a set that holds
//! `number` holds `integer`, so an integer can match two only where two share a bit). Such a
//! union is given those types as its own (`Rules::only_types`), and the walk settles it with
//! one test of the value's kind; where the value fails, the walk reads the union whole, so what
//! it says is unchanged.

use crate::node::{HAS_ARRAY, HAS_COMBINED, HAS_KIND, Id, Items, Node, PlainArray};

/// Every type bit: a branch that allows anything.
const ALL: u8 = 0x7F;
/// How many `$ref`s that are only a `$ref` a branch is followed through.
const ALIASES: usize = 4;

/// Every union of types alone in `nodes` given its types, nested ones first; then every array
/// that asks only types and a count, its plan.
pub(crate) fn fold(nodes: &mut [Node]) {
    unions(nodes);
    // An array of plain arrays is planned once its items are: each round one level more.
    for _ in 0..nodes.len() {
        let plans: Vec<(usize, PlainArray)> = (nodes.iter().enumerate())
            .filter_map(|(at, node)| plain(nodes, node).map(|plan| (at, plan)))
            .collect();
        if plans.is_empty() {
            return;
        }
        for (at, plan) in plans {
            if let Some(Node::Rules(rules)) = nodes.get_mut(at) {
                rules.plain_array = Some(plan);
            }
        }
    }
}

/// The plan of `node`, where it is an array that asks only its type, its count and a type of
/// its items.
fn plain(nodes: &[Node], node: &Node) -> Option<PlainArray> {
    let Node::Rules(rules) = node else {
        return None;
    };
    let alone = rules.only_types.is_none()
        && rules.plain_array.is_none()
        && rules.has & !(HAS_KIND | HAS_ARRAY) == 0
        && rules.reference.is_none()
        && rules.options.is_none()
        && rules.constant.is_none()
        && rules.unevaluated_items.is_none();
    let arrays = rules.array.as_ref().filter(|arrays| {
        alone && arrays.prefix.is_empty() && !arrays.unique && arrays.contains.is_none()
    })?;
    Some(PlainArray {
        types: rules.types.unwrap_or(ALL),
        min: arrays.min_items.unwrap_or(0),
        max: arrays.max_items.unwrap_or(u64::MAX),
        items: match arrays.rest {
            Some(rest) => items(nodes, rest)?,
            None => Items::Types(ALL),
        },
    })
}

/// What the items `rest` asks are, where they are types alone or a planned array.
fn items(nodes: &[Node], rest: Id) -> Option<Items> {
    if let Some(types) = types(nodes, rest) {
        return Some(Items::Types(types));
    }
    let mut id = rest;
    for _ in 0..=ALIASES {
        match nodes.get(id)? {
            Node::Rules(rules) if rules.plain_array.is_some() => return Some(Items::Plain(id)),
            Node::Rules(rules) => id = rules.alias?,
            Node::Any | Node::Never => return None,
        }
    }
    None
}

/// Every union of types alone in `nodes` given its types, nested ones first.
fn unions(nodes: &mut [Node]) {
    // A union of unions folds once its branches have: each round folds one level more.
    for _ in 0..nodes.len() {
        let found: Vec<(usize, u8)> = (nodes.iter().enumerate())
            .filter_map(|(at, node)| folded(nodes, node).map(|types| (at, types)))
            .collect();
        if found.is_empty() {
            return;
        }
        for (at, types) in found {
            if let Some(Node::Rules(rules)) = nodes.get_mut(at) {
                rules.only_types = Some(types);
            }
        }
    }
}

/// The types `node` asks for alone, where it is a union of types not yet folded.
fn folded(nodes: &[Node], node: &Node) -> Option<u8> {
    let Node::Rules(rules) = node else {
        return None;
    };
    // A `$ref` and nothing else asks what it lands on: the walk need not follow it to know.
    if rules.only_types.is_none()
        && let Some(target) = rules.alias
    {
        return types(nodes, target);
    }
    let alone = rules.only_types.is_none()
        && rules.has & !(HAS_COMBINED | HAS_KIND) == 0
        && rules.reference.is_none()
        && rules.options.is_none()
        && rules.constant.is_none()
        && rules.all_of.is_empty()
        && rules.not.is_none()
        && rules.condition.is_none();
    if !alone {
        return None;
    }
    let union = match (rules.any_of.is_empty(), rules.one_of.is_empty()) {
        (false, true) => {
            (rules.any_of.iter()).try_fold(0, |all, &id| Some(all | types(nodes, id)?))?
        }
        (true, false) => (rules.one_of.iter()).try_fold(0, |all: u8, &id| {
            let these = types(nodes, id)?;
            (all & these == 0).then_some(all | these)
        })?,
        (true, true) | (false, false) => return None,
    };
    Some(rules.types.map_or(union, |own| own & union))
}

/// The types branch `id` asks for, where that is all it asks.
fn types(nodes: &[Node], id: Id) -> Option<u8> {
    let mut id = id;
    for _ in 0..=ALIASES {
        match nodes.get(id)? {
            Node::Any => return Some(ALL),
            Node::Never => return Some(0),
            Node::Rules(rules) => match (rules.only_types, rules.alias) {
                (Some(types), _) => return Some(types),
                (None, Some(target)) => id = target,
                (None, None) => return None,
            },
        }
    }
    None
}

#[cfg(test)]
#[path = "kinds_tests.rs"]
mod tests;
