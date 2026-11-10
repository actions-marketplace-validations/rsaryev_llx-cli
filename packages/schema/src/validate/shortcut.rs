//! What the walk settles without walking: a leaf that asks only a type the value has (or a
//! `$ref` to one), and the branches of a tagged union its value can match.

use serde_json::Value;

use super::Walk;
use crate::compile::tags::Tag;
use crate::node::{
    ALL, ARRAY, BOOLEAN, HAS_KIND, HAS_STRING, Id, Items, NULL, NUMBER, Node, OBJECT, PlainArray,
    STRING, kind_of,
};
use crate::report::{At, Report, said};

impl Walk<'_> {
    /// A field's `name` held to `propertyNames`' node `id`, read where it is, with no string
    /// made of it: where the node asks only a string's keywords (a pattern, a length, `false`);
    /// `None` where it asks more.
    pub(super) fn name<const F: bool>(
        &self,
        id: Id,
        name: &str,
        at: &At<'_>,
        mut r: Report<'_, F>,
    ) -> Option<bool> {
        let rules = match self.nodes.get(id) {
            Some(Node::Any) => return Some(true),
            Some(Node::Never) => {
                r.no(|| said!("{at} is not allowed"));
                return Some(false);
            }
            None => return None,
            Some(Node::Rules(rules)) => rules,
        };
        let types_only = rules.has & !(HAS_KIND | HAS_STRING) == 0
            && rules.reference.is_none()
            && rules.options.is_none()
            && rules.constant.is_none();
        if !types_only || rules.types.is_some_and(|types| types & STRING == 0) {
            return None;
        }
        Some(rules.has & HAS_STRING == 0 || self.text(rules, name, at, &mut r))
    }

    /// Whether `value` surely matches node `id` without walking it: the node allows anything,
    /// or asks only for types the value has. `false` says only that the walk must decide.
    #[inline]
    pub(super) fn fits(&self, id: Id, value: &Value) -> bool {
        self.types_of(id)
            .is_some_and(|types| self.has_type(types, value))
    }

    /// Whether `value` matches the array `plan` describes: its type, its count, its items' type.
    #[inline]
    pub(super) fn plain(&self, plan: &PlainArray, value: &Value) -> bool {
        match value {
            Value::Array(items) => {
                let len = items.len() as u64;
                plan.types & ARRAY != 0
                    && (plan.min..=plan.max).contains(&len)
                    && match plan.items {
                        Items::Types(ALL) => true,
                        Items::Types(types) => items.iter().all(|item| self.has_type(types, item)),
                        Items::Plain(id) => match self.nodes.get(id) {
                            Some(Node::Rules(rules)) => {
                                rules.plain_array.as_ref().is_some_and(|inner| {
                                    items.iter().all(|item| self.plain(inner, item))
                                })
                            }
                            Some(Node::Any | Node::Never) | None => false,
                        },
                    }
            }
            Value::Null
            | Value::Bool(_)
            | Value::Number(_)
            | Value::String(_)
            | Value::Object(_) => self.has_type(plan.types, value),
        }
    }

    /// The types node `id` asks for, where that is all it asks (a `$ref` that is all a node
    /// asks was given its target's types as it compiled); every type for one that allows all.
    #[inline(always)]
    pub(super) fn types_of(&self, id: Id) -> Option<u8> {
        match self.nodes.get(id) {
            Some(Node::Any) => Some(ALL),
            Some(Node::Rules(rules)) => rules.only_types,
            Some(Node::Never) | None => None,
        }
    }

    /// Whether `value` is of one of `types`.
    #[inline(always)]
    pub(super) fn has_type(&self, types: u8, value: &Value) -> bool {
        types & self.kind_for(types, value) != 0
    }

    /// The bits of `value`'s kind as `types` reads it: whether a number is whole is read only
    /// where the types tell an integer from a number (a number asked to be any number is one).
    #[inline(always)]
    pub(super) fn kind_for(&self, types: u8, value: &Value) -> u8 {
        match value {
            Value::Null => NULL,
            Value::Bool(_) => BOOLEAN,
            Value::String(_) => STRING,
            Value::Array(_) => ARRAY,
            Value::Object(_) => OBJECT,
            Value::Number(_) if types & NUMBER != 0 => NUMBER,
            Value::Number(_) => kind_of(value, self.strict),
        }
    }
}

/// The branches of a union `value` can match: those its tag leaves, or all of them.
pub(super) fn picked<'a>(
    branches: &'a [Id],
    tag: Option<&'a Tag>,
    value: &'a Value,
) -> impl Iterator<Item = Id> + 'a {
    let (first, second, all) = match tag.and_then(|tag| tag.picks(value)) {
        Some((tagged, untagged)) => (tagged, untagged, &[][..]),
        None => (&[][..], &[][..], branches),
    };
    let place = |at: &u32| {
        usize::try_from(*at)
            .ok()
            .and_then(|at| branches.get(at))
            .copied()
    };
    (first.iter().chain(second).filter_map(place)).chain(all.iter().copied())
}
