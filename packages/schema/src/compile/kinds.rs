//! Types alone, settled once the nodes are built: a `$ref` and nothing else asks the types of
//! what it lands on. Unions of types: an `anyOf` whose branches each ask only for types (`{"type": "string"}`, a
//! `$ref` to one, `true`) asks only for the union of them, and a `oneOf` of such branches too,
//! where no two share a type (a value then matches one of them at most: a set that holds
//! `number` holds `integer`, so an integer can match two only where two share a bit). Such a
//! union is given those types as its own (`Rules::only_types`), and the walk settles it with
//! one test of the value's kind; where the value fails, the walk reads the union whole, so what
//! it says is unchanged.

use super::each;
use crate::node::{ALL, HAS_ARRAY, HAS_COMBINED, HAS_KIND, Id, Items, Node, PlainArray, Rules};

/// How many `$ref`s that are only a `$ref` a branch is followed through.
const ALIASES: usize = 4;

/// Every union of types alone in `nodes` given its types, nested ones first; then every array
/// that asks only types and a count, its plan.
pub(crate) fn fold(nodes: &mut [Node]) {
    // A union of unions folds once its branches have, and an array of plain arrays is planned
    // once its items are: each round one level more.
    for _ in 0..nodes.len() {
        if !each(nodes, folded, |rules, types| rules.only_types = Some(types)) {
            break;
        }
    }
    for _ in 0..nodes.len() {
        if !each(nodes, plain, |rules, plan| rules.plain_array = Some(plan)) {
            break;
        }
    }
}

/// The plan of `rules`, where they are an array's that asks only its type, its count and a
/// type of its items.
fn plain(nodes: &[Node], rules: &Rules) -> Option<PlainArray> {
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

/// The types `rules` ask for alone, where they are a union of types not yet folded.
fn folded(nodes: &[Node], rules: &Rules) -> Option<u8> {
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
