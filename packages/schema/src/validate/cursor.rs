//! The fields `properties` names, met as a data object's fields come: both in the order of
//! their names, so a cursor finds each where it left the last.

use std::cmp::Ordering;

use serde_json::{Map, Value};

use crate::node::Id;

/// The fields `properties` names, met in the order of their names: a data object's fields come
/// in that order (serde_json's map), so each is found where the last one was left, most in one
/// comparison; one out of that order is searched for.
pub(super) struct Cursor<'p, 'd> {
    properties: &'p [(Box<str>, Id, u64)],
    next: usize,
    /// The `required` bits of the properties gone past with no field met.
    pub(super) passed: u64,
    /// The last field's name: a name after it comes in order, and is not behind the cursor.
    last: &'d str,
}

impl<'p, 'd> Cursor<'p, 'd> {
    /// A cursor before the first of `properties`.
    pub(super) fn new(properties: &'p [(Box<str>, Id, u64)]) -> Self {
        Cursor {
            properties,
            next: 0,
            passed: 0,
            last: "",
        }
    }

    /// The node of the field `name`, with its `required` bit.
    pub(super) fn find(&mut self, name: &'d str) -> Option<(Id, u64)> {
        let in_order = order(name, self.last) == Ordering::Greater;
        self.last = name;
        while let Some((known, id, bit)) = self.properties.get(self.next) {
            match order(known, name) {
                Ordering::Less => {
                    self.passed |= bit;
                    self.next += 1;
                }
                Ordering::Equal => {
                    self.next += 1;
                    return Some((*id, *bit));
                }
                Ordering::Greater => break,
            }
        }
        // In order, every property behind the cursor is before an earlier name: none is this one.
        if in_order {
            return None;
        }
        let place = (self.properties)
            .binary_search_by(|(known, _, _)| order(known, name))
            .ok()?;
        self.properties.get(place).map(|(_, id, bit)| (*id, *bit))
    }
}

impl Cursor<'_, '_> {
    /// Whether a required property gone past unmet is not in `fields` (it may have come out of
    /// order); those found are not asked again.
    pub(super) fn missing(&mut self, fields: &Map<String, Value>, met: u64) -> bool {
        let lost = self.passed & !met;
        self.passed &= met;
        (self.properties.iter())
            .filter(|(_, _, bit)| lost & bit != 0)
            .any(|(name, _, _)| !fields.contains_key(&**name))
    }
}

/// Two names in byte order, eight bytes at a time and inline: a field's name is short, and a
/// call to the system's `memcmp` for each costs more than the comparison.
#[inline]
pub(super) fn order(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.as_bytes(), b.as_bytes());
    while let (Some((x, rest_a)), Some((y, rest_b))) =
        (a.split_first_chunk::<8>(), b.split_first_chunk::<8>())
    {
        let (x, y) = (u64::from_be_bytes(*x), u64::from_be_bytes(*y));
        if x != y {
            return x.cmp(&y);
        }
        (a, b) = (rest_a, rest_b);
    }
    for (x, y) in a.iter().zip(b) {
        if x != y {
            return x.cmp(y);
        }
    }
    a.len().cmp(&b.len())
}

#[cfg(test)]
#[path = "cursor_tests.rs"]
mod tests;
