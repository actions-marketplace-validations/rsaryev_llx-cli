//! What `unevaluatedProperties` and `unevaluatedItems` find looked at whatever the data: where
//! the schema itself, or what it applies to every value it allows (its `$ref`s, its `allOf`,
//! theirs in turn), takes every field (`additionalProperties`, a nested
//! `unevaluatedProperties`) or every item (`items` past the prefix, a nested
//! `unevaluatedItems`), nothing is left unevaluated, and the walk need not work it out. The
//! walk marks those parts whatever they find (`validate/unevaluated.rs`), so this is the same
//! answer, known as the schema compiles.

use crate::node::{Node, Rules};

/// How deep `$ref`s and `allOf`s are followed: a loop of them settles nothing.
const DEPTH: usize = 16;

/// Which of a data value's parts are counted.
#[derive(Clone, Copy)]
enum Of {
    Fields,
    Items,
}

/// Each node that asks `unevaluated*` told whether its own parts take every field or item.
pub(crate) fn settle(nodes: &mut [Node]) {
    let settled: Vec<(usize, bool, bool)> = (nodes.iter().enumerate())
        .filter_map(|(at, node)| match node {
            Node::Rules(rules)
                if rules.unevaluated_fields.is_some() || rules.unevaluated_items.is_some() =>
            {
                let fields = takes_all(nodes, rules, Of::Fields, DEPTH, Asker::Itself);
                let items = takes_all(nodes, rules, Of::Items, DEPTH, Asker::Itself);
                Some((at, fields, items))
            }
            Node::Rules(_) | Node::Any | Node::Never => None,
        })
        .collect();
    for (at, fields, items) in settled {
        if let Some(Node::Rules(rules)) = nodes.get_mut(at) {
            rules.every_field_seen = fields;
            rules.every_item_seen = items;
        }
    }
}

/// Whether a schema's own `unevaluated*` counts: not for the one that asks it, which is
/// working out what the rest looked at; yes for one it applies, which looks at everything.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Asker {
    Itself,
    Applied,
}

/// Whether `rules`, or what they apply to every value, take every field or item of one.
fn takes_all(nodes: &[Node], rules: &Rules, of: Of, depth: usize, asker: Asker) -> bool {
    let nested = asker == Asker::Applied;
    let here = match of {
        Of::Fields => {
            (rules.object.as_ref()).is_some_and(|objects| objects.extra.applies())
                || (nested && rules.unevaluated_fields.is_some())
        }
        Of::Items => {
            (rules.array.as_ref()).is_some_and(|arrays| arrays.rest.is_some())
                || (nested && rules.unevaluated_items.is_some())
        }
    };
    here || depth > 0
        && (rules.reference.iter().chain(rules.all_of.iter())).any(|id| match nodes.get(*id) {
            Some(Node::Rules(applied)) => takes_all(nodes, applied, of, depth - 1, Asker::Applied),
            Some(Node::Any | Node::Never) | None => false,
        })
}

#[cfg(test)]
#[path = "evaluated_tests.rs"]
mod tests;
