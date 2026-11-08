//! A hash of JSON that agrees with its equality (`1` and `1.0` hash alike; an object's fields in
//! any order), for what compares a value with many: `uniqueItems` over a long list, and `enum`
//! with many options, each a table looked up once instead of every option compared.

use serde_json::Value;

use crate::number::{exact, json_eq, whole};

/// Lists this long or shorter are compared item by item: hashing costs more than it saves.
const SHORT: usize = 16;

/// The hash of `value`.
pub(crate) fn hash(value: &Value) -> u64 {
    match value {
        Value::Null => mix(1, 0),
        Value::Bool(b) => mix(2, u64::from(*b)),
        Value::Number(n) => {
            let float = n.as_f64().unwrap_or(0.0);
            match whole(n).or_else(|| exact(float)) {
                Some(i) => {
                    let [low, high] = halves(i);
                    mix(mix(3, low), high)
                }
                None => mix(4, float.to_bits()),
            }
        }
        Value::String(text) => bytes(5, text.as_bytes()),
        Value::Array(items) => items
            .iter()
            .fold(mix(6, items.len() as u64), |h, item| mix(h, hash(item))),
        // A sum of each field's hash: the same whatever order the fields come in.
        Value::Object(fields) => fields
            .iter()
            .fold(mix(7, fields.len() as u64), |h, (k, v)| {
                h.wrapping_add(mix(bytes(8, k.as_bytes()), hash(v)))
            }),
    }
}

/// An integer's two words, low first.
fn halves(i: i128) -> [u64; 2] {
    let bytes = i.to_le_bytes();
    let mut words = [0; 2];
    for (word, chunk) in words.iter_mut().zip(bytes.as_chunks::<8>().0) {
        *word = u64::from_le_bytes(*chunk);
    }
    words
}

fn mix(h: u64, word: u64) -> u64 {
    (h ^ word)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .rotate_left(29)
        .wrapping_add(0x632B_E59B_D9B4_E019)
}

fn bytes(seed: u64, text: &[u8]) -> u64 {
    let (chunks, rest) = text.as_chunks::<8>();
    let mut h = mix(seed, text.len() as u64);
    for chunk in chunks {
        h = mix(h, u64::from_le_bytes(*chunk));
    }
    let mut tail = [0; 8];
    if let Some(room) = tail.get_mut(..rest.len()) {
        room.copy_from_slice(rest);
    }
    mix(h, u64::from_le_bytes(tail))
}

/// The first item that repeats an earlier one, with the earliest it repeats, by their places.
pub(crate) fn duplicate(items: &[Value]) -> Option<(usize, usize)> {
    if items.len() <= SHORT {
        return (items.iter().enumerate().skip(1)).find_map(|(second, b)| {
            (items.iter().take(second))
                .position(|a| json_eq(a, b))
                .map(|first| (first, second))
        });
    }
    // An open-addressed set of the items seen, each slot a hash and a place plus one: one pass,
    // each item looked up once, no sort.
    let size = items.len().checked_mul(2)?.checked_next_power_of_two()?;
    let mask = size - 1;
    let mut slots: Vec<(u64, usize)> = vec![(0, 0); size];
    for (second, item) in items.iter().enumerate() {
        let h = hash(item);
        let mut at = slot(h, mask);
        loop {
            let slot = slots.get_mut(at)?;
            match *slot {
                (_, 0) => {
                    *slot = (h, second + 1);
                    break;
                }
                (seen, first) if seen == h && same(items, first - 1, second) => {
                    return Some((first - 1, second));
                }
                _ => at = (at + 1) & mask,
            }
        }
    }
    None
}

/// The slot of hash `h` in a table of `mask + 1` slots, a power of two. Fibonacci hashing: the
/// product's top bits, spread however alike the hashes are (short strings differ in few bits).
fn slot(h: u64, mask: usize) -> usize {
    let bits = mask.count_ones();
    let top = match bits {
        0 => 0,
        bits => h.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> (64 - bits),
    };
    usize::try_from(top).unwrap_or(0) & mask
}

fn same(items: &[Value], a: usize, b: usize) -> bool {
    matches!((items.get(a), items.get(b)), (Some(a), Some(b)) if json_eq(a, b))
}

/// `enum`'s options as a table of their hashes, open addressed: a value is looked up once.
pub(crate) struct Table {
    /// Each slot a hash and its option's place plus one; none for an empty slot.
    slots: Box<[(u64, u32)]>,
}

impl Table {
    /// A table of `options`, or none where they are few enough to compare one by one.
    pub(crate) fn of(options: &[Value]) -> Option<Self> {
        let len = u32::try_from(options.len()).ok()?;
        if options.len() <= SHORT / 2 {
            return None;
        }
        let mut slots = vec![(0, 0); (options.len() * 2).next_power_of_two()].into_boxed_slice();
        let mask = slots.len() - 1;
        for (place, option) in (1..=len).zip(options) {
            let h = hash(option);
            let mut at = slot(h, mask);
            while slots.get(at).is_some_and(|slot| slot.1 != 0) {
                at = (at + 1) & mask;
            }
            if let Some(slot) = slots.get_mut(at) {
                *slot = (h, place);
            }
        }
        Some(Table { slots })
    }

    /// Whether `value` is one of `options` (those the table was made of).
    pub(crate) fn holds(&self, options: &[Value], value: &Value) -> bool {
        let h = hash(value);
        let mask = self.slots.len() - 1;
        let mut at = slot(h, mask);
        while let Some((slot_hash, place)) = self.slots.get(at).filter(|slot| slot.1 != 0) {
            let option = options.get(usize::try_from(*place).unwrap_or_default().wrapping_sub(1));
            if *slot_hash == h && option.is_some_and(|option| json_eq(option, value)) {
                return true;
            }
            at = (at + 1) & mask;
        }
        false
    }
}

/// Names looked up by their hash, open addressed: `properties` past a few, where walking them
/// beside the data's fields costs more than one lookup a field.
pub(crate) struct Names {
    /// Each slot a name's place among the properties plus one; none for an empty slot.
    slots: Box<[u32]>,
}

impl Names {
    /// Past this many properties a field is looked up by hash.
    pub(crate) const PAST: usize = 12;

    pub(crate) fn of<T>(names: &[(Box<str>, T, u64)]) -> Option<Self> {
        if names.len() <= Self::PAST {
            return None;
        }
        let len = u32::try_from(names.len()).ok()?;
        let mut slots = vec![0; (names.len() * 2).next_power_of_two()].into_boxed_slice();
        let mask = slots.len() - 1;
        for (place, (name, _, _)) in (1..=len).zip(names) {
            let mut at = slot(bytes(9, name.as_bytes()), mask);
            while slots.get(at).is_some_and(|slot| *slot != 0) {
                at = (at + 1) & mask;
            }
            if let Some(slot) = slots.get_mut(at) {
                *slot = place;
            }
        }
        Some(Names { slots })
    }

    /// The place of `name` among `names` (those the table was made of).
    pub(crate) fn find<T>(&self, names: &[(Box<str>, T, u64)], name: &str) -> Option<usize> {
        let mask = self.slots.len() - 1;
        let mut at = slot(bytes(9, name.as_bytes()), mask);
        while let Some(place) = self.slots.get(at).filter(|slot| **slot != 0) {
            let place = usize::try_from(*place).unwrap_or_default().wrapping_sub(1);
            if names
                .get(place)
                .is_some_and(|(known, _, _)| **known == *name)
            {
                return Some(place);
            }
            at = (at + 1) & mask;
        }
        None
    }
}

#[cfg(test)]
#[path = "hash_tests.rs"]
mod tests;
