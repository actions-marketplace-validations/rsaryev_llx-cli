//! `unevaluatedProperties` and `unevaluatedItems`: what no other keyword of the schema looked at,
//! counting what its in-place subschemas that hold looked at (`allOf`, the branches of `anyOf` and
//! `oneOf` that match, `if` with the branch it chose, `$ref`, `dependentSchemas`, and a nested
//! `unevaluated*`, which looks at everything). Worked out only for a schema that asks it.

use serde_json::{Map, Value};

use super::cursor::Cursor;
use super::{DEPTH, Walk};
use crate::node::{Id, Node, Rules};
use crate::report::{At, Report, said};

/// The fields or items whose marks are kept on the stack: an object or array of more takes them
/// on the heap.
const ON_STACK: usize = 64;

/// A mark for each of `len` fields or items, all unset: on `stack` where they fit.
fn marks<'m>(
    stack: &'m mut [bool; ON_STACK],
    heap: &'m mut Vec<bool>,
    len: usize,
) -> &'m mut [bool] {
    match stack.get_mut(..len) {
        Some(short) => short,
        None => {
            heap.resize(len, false);
            heap
        }
    }
}

/// What a walk marks as looked at: a data object's fields, or an array's items, by place.
#[derive(Clone, Copy)]
enum Of<'v> {
    Fields(&'v Map<String, Value>),
    Items(&'v [Value]),
}

impl Walk<'_> {
    pub(super) fn unevaluated_fields<const F: bool>(
        &self,
        rules: &Rules,
        object: &Value,
        fields: &Map<String, Value>,
        at: &At<'_>,
        r: &mut Report<'_, F>,
        refs: usize,
    ) -> bool {
        let Some(rest) = rules.unevaluated_fields else {
            return true;
        };
        let (mut stack, mut heap) = ([false; ON_STACK], vec![]);
        let seen = marks(&mut stack, &mut heap, fields.len());
        self.mark(rules, object, Of::Fields(fields), seen, refs, Own::Skip);
        for ((name, value), _) in fields.iter().zip(seen.iter()).filter(|(_, seen)| !**seen) {
            let ok = match self.nodes.get(rest) {
                Some(Node::Never) => {
                    r.no(|| said!("{at} has \"{name}\", which the schema does not allow"))
                }
                Some(Node::Any | Node::Rules(_)) | None => {
                    self.node(rest, value, &At::Key(at, name), r.part(), 0)
                }
            };
            if !r.took(ok) {
                return false;
            }
        }
        true
    }

    pub(super) fn unevaluated_items<const F: bool>(
        &self,
        rules: &Rules,
        array: &Value,
        items: &[Value],
        at: &At<'_>,
        r: &mut Report<'_, F>,
        refs: usize,
    ) -> bool {
        let Some(rest) = rules.unevaluated_items else {
            return true;
        };
        let (mut stack, mut heap) = ([false; ON_STACK], vec![]);
        let seen = marks(&mut stack, &mut heap, items.len());
        self.mark(rules, array, Of::Items(items), seen, refs, Own::Skip);
        for (index, (item, _)) in items
            .iter()
            .zip(seen.iter())
            .enumerate()
            .filter(|(_, (_, s))| !**s)
        {
            let ok = self.node(rest, item, &At::Index(at, index), r.part(), 0);
            if !r.took(ok) {
                return false;
            }
        }
        true
    }

    fn mark_node(&self, id: Id, value: &Value, of: Of<'_>, seen: &mut [bool], refs: usize) {
        if let Some(Node::Rules(rules)) = self.nodes.get(id) {
            self.mark(rules, value, of, seen, refs, Own::Count);
        }
    }

    /// Marks in `seen` what `rules`, which hold for `value`, look at.
    fn mark(
        &self,
        rules: &Rules,
        value: &Value,
        of: Of<'_>,
        seen: &mut [bool],
        refs: usize,
        own: Own,
    ) {
        if refs > DEPTH {
            return;
        }
        if let Some(target) = rules.reference {
            self.mark_node(target, value, of, seen, refs + 1);
        }
        let nested = match of {
            Of::Fields(_) => rules.unevaluated_fields.is_some(),
            Of::Items(_) => rules.unevaluated_items.is_some(),
        };
        if nested && own == Own::Count {
            seen.iter_mut().for_each(|seen| *seen = true);
            return;
        }
        self.looked_at(rules, of, seen);
        let holds = |id: &Id| self.quick(*id, value, &At::Root, refs);
        let branches = (rules.any_of.iter().filter(|id| holds(id)))
            .chain(rules.one_of.iter().filter(|id| holds(id)))
            .chain(rules.all_of.iter());
        for id in branches {
            self.mark_node(*id, value, of, seen, refs);
        }
        if let Some((when, then, otherwise)) = rules.condition {
            let branch = match holds(&when) {
                true => {
                    self.mark_node(when, value, of, seen, refs);
                    then
                }
                false => otherwise,
            };
            if let Some(id) = branch {
                self.mark_node(id, value, of, seen, refs);
            }
        }
        if let (Of::Fields(fields), Some(objects)) = (of, &rules.object) {
            for (name, id) in &objects.dependent_schemas {
                if fields.contains_key(&**name) {
                    self.mark_node(*id, value, of, seen, refs);
                }
            }
        }
    }

    /// What `rules`' own keywords look at.
    fn looked_at(&self, rules: &Rules, of: Of<'_>, seen: &mut [bool]) {
        match of {
            Of::Fields(fields) => {
                let Some(objects) = &rules.object else {
                    return;
                };
                let every = objects.extra.applies();
                // The fields and `properties` are met in one pass, as the walk meets them.
                let mut cursor = Cursor::new(&objects.properties);
                for ((name, _), seen) in fields.iter().zip(seen.iter_mut()) {
                    let named = !every && cursor.find(name).is_some();
                    let patterned = !every
                        && !named
                        && (objects.patterns.iter()).any(|(p, _)| self.matches(*p, name));
                    *seen |= every || named || patterned;
                }
            }
            Of::Items(items) => {
                let Some(arrays) = &rules.array else {
                    return;
                };
                let upto = match arrays.rest {
                    Some(_) => items.len(),
                    None => arrays.prefix.len(),
                };
                for (index, seen) in seen.iter_mut().enumerate() {
                    let contained = arrays.contains.is_some_and(|(id, _, _)| {
                        let item = items.get(index);
                        item.is_some_and(|item| self.quick(id, item, &At::Root, 0))
                    });
                    *seen |= index < upto || contained;
                }
            }
        }
    }
}

/// Whether a schema's own `unevaluated*` counts: not for the schema that asks it, which works
/// out what the rest looked at; yes for one nested in it, which looks at everything.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Own {
    Skip,
    Count,
}
