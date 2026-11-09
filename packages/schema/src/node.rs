//! The compiled schema's nodes: what each schema object asks, in the shape a walk reads with no
//! keyword looked up (`compile.rs` builds them, `validate.rs` walks them).

use std::cmp::Ordering;

use serde_json::{Number, Value};

use crate::hash::Table;
use crate::number;

/// A node's place in the compiled schema.
pub(crate) type Id = usize;

/// Field names, as `required` and `dependentRequired` list them.
pub(crate) type Names = Box<[Box<str>]>;

/// A schema: `true` (or anything that is not an object), `false`, or an object's keywords.
pub(crate) enum Node {
    Any,
    Never,
    Rules(Box<Rules>),
}

/// What one schema object asks. Each keyword it lacks is empty here, and costs nothing.
#[derive(Default)]
pub(crate) struct Rules {
    /// Which groups of keywords this node has (`HAS_*`): a walk skips the rest unread.
    pub(crate) has: u8,
    pub(crate) reference: Option<Id>,
    /// `type` as `kind_of`'s bits; `None` with no `type`.
    pub(crate) types: Option<u8>,
    /// `type`'s names, as a message says them.
    pub(crate) type_names: Names,
    /// `enum` as written: its options, and the whole for a message; with many options, the table
    /// a value is looked up in.
    pub(crate) options: Option<Value>,
    pub(crate) table: Option<Table>,
    /// The kinds among `enum`'s options: a value of no such kind is none of them, unread.
    pub(crate) option_kinds: u8,
    pub(crate) constant: Option<Value>,
    pub(crate) all_of: Box<[Id]>,
    pub(crate) any_of: Box<[Id]>,
    pub(crate) one_of: Box<[Id]>,
    pub(crate) not: Option<Id>,
    /// `if`, with `then` and `else`.
    pub(crate) condition: Option<(Id, Option<Id>, Option<Id>)>,
    /// The number bounds in the order they are said, and `multipleOf`.
    pub(crate) limits: Box<[(Number, Sign)]>,
    pub(crate) multiple: Option<Number>,
    pub(crate) min_length: Option<u64>,
    pub(crate) max_length: Option<u64>,
    /// `pattern`: its place among the schema's patterns.
    pub(crate) pattern: Option<usize>,
    /// `format`, where it is one this check knows: its name, for a message, and its check.
    pub(crate) format: Option<(Box<str>, crate::format::Check)>,
    pub(crate) array: Option<Box<Arrays>>,
    pub(crate) object: Option<Box<Objects>>,
    /// `unevaluatedProperties` and `unevaluatedItems`: what nothing else here looked at is held to.
    pub(crate) unevaluated_fields: Option<Id>,
    pub(crate) unevaluated_items: Option<Id>,
    /// The tags of `anyOf` and `oneOf`, where a field settles their branches.
    pub(crate) any_tag: Option<Box<crate::compile::tags::Tag>>,
    pub(crate) one_tag: Option<Box<crate::compile::tags::Tag>>,
    /// The types, where they are all the schema asks (`{"type": "string"}`, most leaves): the
    /// walk checks such a value where it stands, with no call.
    pub(crate) only_types: Option<u8>,
    /// The node this one only refers to, where a `$ref` is all it asks.
    pub(crate) alias: Option<Id>,
    /// An array that asks only its type, its count and its items' types: the quick walk reads
    /// it in one loop (`compile/kinds.rs`).
    pub(crate) plain_array: Option<PlainArray>,
    /// Whether what the node applies to every value it allows takes every field, or every
    /// item: `unevaluated*` then finds nothing left (`compile/evaluated.rs`).
    pub(crate) every_field_seen: bool,
    pub(crate) every_item_seen: bool,
}

/// What an array that asks only its type, its count and its items' types asks.
pub(crate) struct PlainArray {
    /// The node's own types (every one where it says none).
    pub(crate) types: u8,
    pub(crate) min: u64,
    pub(crate) max: u64,
    pub(crate) items: Items,
}

/// What each item of a plain array must be.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Items {
    /// Of these types (every one where `items` says nothing).
    Types(u8),
    /// A plain array itself, as the node's plan says.
    Plain(Id),
}

impl Rules {
    /// These rules with every keyword of the validation vocabulary taken out, for a schema whose
    /// metaschema leaves that vocabulary out: they then ask only what the applicators do.
    pub(crate) fn without_validation(&mut self) {
        (self.types, self.type_names) = (None, Box::default());
        (self.options, self.table, self.option_kinds, self.constant) = (None, None, 0, None);
        (self.limits, self.multiple) = (Box::default(), None);
        (self.min_length, self.max_length, self.pattern) = (None, None, None);
        if let Some(arrays) = &mut self.array {
            (arrays.min_items, arrays.max_items, arrays.unique) = (None, None, false);
            if let Some(contains) = &mut arrays.contains {
                (contains.1, contains.2) = (1, None);
            }
        }
        if let Some(objects) = &mut self.object {
            (objects.required, objects.required_bits) = (Box::default(), None);
            (objects.min_fields, objects.max_fields) = (None, None);
            objects.dependent = Box::default();
            objects.closing = !objects.dependent_schemas.is_empty();
        }
        self.has = self.groups();
    }
}

/// The groups of keywords a node may have.
pub(crate) const HAS_KIND: u8 = 1;
pub(crate) const HAS_COMBINED: u8 = 1 << 1;
pub(crate) const HAS_NUMBER: u8 = 1 << 2;
pub(crate) const HAS_STRING: u8 = 1 << 3;
pub(crate) const HAS_ARRAY: u8 = 1 << 4;
pub(crate) const HAS_OBJECT: u8 = 1 << 5;

impl Rules {
    /// The groups of keywords these rules have.
    pub(crate) fn groups(&self) -> u8 {
        let kind = self.types.is_some() || self.options.is_some() || self.constant.is_some();
        let combined = !self.all_of.is_empty()
            || !self.any_of.is_empty()
            || !self.one_of.is_empty()
            || self.not.is_some()
            || self.condition.is_some();
        let number = !self.limits.is_empty() || self.multiple.is_some();
        let string = self.min_length.is_some()
            || self.max_length.is_some()
            || self.pattern.is_some()
            || self.format.is_some();
        let array = self.array.is_some() || self.unevaluated_items.is_some();
        let object = self.object.is_some() || self.unevaluated_fields.is_some();
        [
            (kind, HAS_KIND),
            (combined, HAS_COMBINED),
            (number, HAS_NUMBER),
            (string, HAS_STRING),
            (array, HAS_ARRAY),
            (object, HAS_OBJECT),
        ]
        .into_iter()
        .filter(|(present, _)| *present)
        .fold(0, |all, (_, bit)| all | bit)
    }
}

/// How a number must stand to a bound.
#[derive(Clone, Copy)]
pub(crate) enum Sign {
    AtLeast,
    Above,
    AtMost,
    Below,
}

impl Sign {
    /// Whether `n` stands so to `limit`, compared exactly.
    pub(crate) fn holds(self, n: &Number, limit: &Number) -> bool {
        let Some(order) = number::compare(n, limit) else {
            return false;
        };
        match self {
            Sign::AtLeast => order != Ordering::Less,
            Sign::Above => order == Ordering::Greater,
            Sign::AtMost => order != Ordering::Greater,
            Sign::Below => order == Ordering::Less,
        }
    }

    pub(crate) fn said(self) -> &'static str {
        match self {
            Sign::AtLeast => ">=",
            Sign::Above => ">",
            Sign::AtMost => "<=",
            Sign::Below => "<",
        }
    }
}

/// The keywords of an array.
pub(crate) struct Arrays {
    pub(crate) prefix: Box<[Id]>,
    pub(crate) rest: Option<Id>,
    pub(crate) min_items: Option<u64>,
    pub(crate) max_items: Option<u64>,
    pub(crate) unique: bool,
    /// `contains`, with `minContains` (1 when not said) and `maxContains`.
    pub(crate) contains: Option<(Id, u64, Option<u64>)>,
}

/// The keywords of an object.
pub(crate) struct Objects {
    pub(crate) required: Names,
    /// `properties` in the order of their names, each with the bit of its place in `required`
    /// (none when not required or past the 64th): a check that meets them all in the data looks
    /// none of `required` up.
    pub(crate) properties: Box<[(Box<str>, Id, u64)]>,
    /// `properties` by hash, where there are many.
    pub(crate) lookup: Option<crate::hash::Names>,
    /// The bits of every required field `properties` names, when they are all of `required`.
    pub(crate) required_bits: Option<u64>,
    /// `patternProperties`: each pattern's place among the schema's patterns, and its node.
    pub(crate) patterns: Box<[(usize, Id)]>,
    pub(crate) extra: Extra,
    /// `propertyNames`.
    pub(crate) names: Option<Id>,
    pub(crate) min_fields: Option<u64>,
    pub(crate) max_fields: Option<u64>,
    pub(crate) dependent: Box<[(Box<str>, Names)]>,
    /// `dependentSchemas` (or draft 7's `dependencies` given a schema).
    pub(crate) dependent_schemas: Box<[(Box<str>, Id)]>,
    /// Whether any of the counts or the dependencies is said: an object is held to them after
    /// its fields.
    pub(crate) closing: bool,
}

/// What a field neither `properties` nor `patternProperties` names is held to.
pub(crate) enum Extra {
    /// No `additionalProperties`: anything, and it looks at none of them.
    Absent,
    /// `additionalProperties: true`: anything, and it looks at all of them.
    Allowed,
    Refused,
    Checked(Id),
}

impl Extra {
    pub(crate) fn applies(&self) -> bool {
        match self {
            Extra::Absent => false,
            Extra::Allowed | Extra::Refused | Extra::Checked(_) => true,
        }
    }
}

/// The types of JSON Schema as bits of a value's kind.
pub(crate) const NULL: u8 = 1;
pub(crate) const BOOLEAN: u8 = 1 << 1;
pub(crate) const INTEGER: u8 = 1 << 2;
pub(crate) const NUMBER: u8 = 1 << 3;
pub(crate) const STRING: u8 = 1 << 4;
pub(crate) const ARRAY: u8 = 1 << 5;
pub(crate) const OBJECT: u8 = 1 << 6;
/// Every type's bit: what a node that allows anything allows.
pub(crate) const ALL: u8 = NULL | BOOLEAN | INTEGER | NUMBER | STRING | ARRAY | OBJECT;

/// The bits of `value`'s kinds: a number with no fraction is an integer too (only one written
/// so, where `strict`: draft 4).
pub(crate) fn kind_of(value: &Value, strict: bool) -> u8 {
    match value {
        Value::Null => NULL,
        Value::Bool(_) => BOOLEAN,
        Value::Number(_) if strict && number::is_written_integer(value) => NUMBER | INTEGER,
        Value::Number(_) if !strict && number::is_integer(value) => NUMBER | INTEGER,
        Value::Number(_) => NUMBER,
        Value::String(_) => STRING,
        Value::Array(_) => ARRAY,
        Value::Object(_) => OBJECT,
    }
}
