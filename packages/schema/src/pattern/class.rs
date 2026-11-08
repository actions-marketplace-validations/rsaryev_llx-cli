//! A set of characters a pattern takes at one place: a literal, `.`, `[...]`, `\d`, as sorted
//! ranges of Unicode scalar values, with ECMA-262's meaning of each class escape.

/// The last code point; surrogates are inside the range but no `char` is one, so a range across
/// them matches what it should.
const MAX: u32 = 0x10_FFFF;

/// ECMA-262's `\s`: WhiteSpace (tab, VT, FF, space, NBSP, BOM and every Zs) and LineTerminator.
const SPACES: [(u32, u32); 10] = [
    (0x09, 0x0D),
    (0x20, 0x20),
    (0xA0, 0xA0),
    (0x1680, 0x1680),
    (0x2000, 0x200A),
    (0x2028, 0x2029),
    (0x202F, 0x202F),
    (0x205F, 0x205F),
    (0x3000, 0x3000),
    (0xFEFF, 0xFEFF),
];

/// Characters as sorted, disjoint, non-adjacent ranges of code points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Class {
    ranges: Vec<(u32, u32)>,
}

impl Class {
    /// No character: `[]`.
    pub(super) fn empty() -> Self {
        Self { ranges: vec![] }
    }

    pub(super) fn one(c: char) -> Self {
        Self::of(&[(u32::from(c), u32::from(c))])
    }

    /// `first-last`, or `None` when the range is out of order.
    pub(super) fn range(first: char, last: char) -> Option<Self> {
        (first <= last).then(|| Self::of(&[(u32::from(first), u32::from(last))]))
    }

    /// `\d`: ASCII digits only, as ECMA-262 says.
    pub(super) fn digit() -> Self {
        Self::of(&[(0x30, 0x39)])
    }

    /// `\w`: `[A-Za-z0-9_]`, ASCII only.
    pub(super) fn word() -> Self {
        Self::of(&[(0x30, 0x39), (0x41, 0x5A), (0x5F, 0x5F), (0x61, 0x7A)])
    }

    /// `\s`: ECMA-262's white space and line terminators, Unicode's spaces among them.
    pub(super) fn space() -> Self {
        Self::of(&SPACES)
    }

    /// `\p{name}` for the properties whose characters are few and fixed: `Zs` (the spaces),
    /// `ASCII`, `ASCII_Hex_Digit`, `Any`; `None` for the rest, which need Unicode's tables.
    pub(super) fn property(name: &str) -> Option<Self> {
        let name = name
            .strip_prefix("General_Category=")
            .or_else(|| name.strip_prefix("gc="))
            .unwrap_or(name);
        let ranges: &[(u32, u32)] = match name {
            "Zs" | "Space_Separator" => &[
                (0x20, 0x20),
                (0xA0, 0xA0),
                (0x1680, 0x1680),
                (0x2000, 0x200A),
                (0x202F, 0x202F),
                (0x205F, 0x205F),
                (0x3000, 0x3000),
            ],
            "ASCII" => &[(0, 0x7F)],
            "ASCII_Hex_Digit" | "AHex" => &[(0x30, 0x39), (0x41, 0x46), (0x61, 0x66)],
            "Any" => &[(0, MAX)],
            _ => return None,
        };
        Some(Self::of(ranges))
    }

    /// `.`: every character but the line terminators.
    pub(super) fn dot() -> Self {
        Self::of(&[(0x0A, 0x0A), (0x0D, 0x0D), (0x2028, 0x2029)]).negated()
    }

    fn of(ranges: &[(u32, u32)]) -> Self {
        let mut class = Self::empty();
        for &(first, last) in ranges {
            class.add(first, last);
        }
        class
    }

    fn add(&mut self, first: u32, last: u32) {
        self.ranges.push((first, last));
        self.ranges.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(self.ranges.len());
        for &(a, b) in &self.ranges {
            match merged.last_mut() {
                Some(top) if a <= top.1.saturating_add(1) => top.1 = top.1.max(b),
                Some(_) | None => merged.push((a, b)),
            }
        }
        self.ranges = merged;
    }

    /// Every character of `other` added.
    pub(super) fn union(&mut self, other: &Self) {
        for &(first, last) in &other.ranges {
            self.add(first, last);
        }
    }

    /// Every character this class does not hold.
    #[must_use]
    pub(super) fn negated(&self) -> Self {
        let mut out = vec![];
        let mut next = 0;
        for &(first, last) in &self.ranges {
            if first > next {
                out.push((next, first - 1));
            }
            next = last.saturating_add(1);
        }
        if next <= MAX {
            out.push((next, MAX));
        }
        Self { ranges: out }
    }

    pub(super) fn contains(&self, c: char) -> bool {
        let c = u32::from(c);
        let at = self.ranges.partition_point(|&(_, last)| last < c);
        self.ranges.get(at).is_some_and(|&(first, _)| first <= c)
    }

    /// The ASCII characters held, bit `c` for character `c`.
    pub(super) fn ascii(&self) -> u128 {
        let mut bits = 0u128;
        for &(first, last) in &self.ranges {
            for c in first..=last.min(0x7F) {
                bits |= 1 << c;
            }
        }
        bits
    }

    /// Whether a character past ASCII is held.
    pub(super) fn beyond_ascii(&self) -> bool {
        self.ranges.last().is_some_and(|&(_, last)| last > 0x7F)
    }

    /// The one character this class holds, when it holds exactly one.
    pub(super) fn single(&self) -> Option<char> {
        match self.ranges.as_slice() {
            [(first, last)] if first == last => char::from_u32(*first),
            _ => None,
        }
    }

    /// The ranges as characters, cut around the surrogates no `char` can be.
    pub(super) fn char_ranges(&self) -> Vec<(char, char)> {
        let mut out = vec![];
        for &(first, last) in &self.ranges {
            for (a, b) in [(first, last.min(0xD7FF)), (first.max(0xE000), last)] {
                if let (true, Some(a), Some(b)) = (a <= b, char::from_u32(a), char::from_u32(b)) {
                    out.push((a, b));
                }
            }
        }
        out
    }
}

#[cfg(test)]
#[path = "class_tests.rs"]
mod tests;
