//! A set of an automaton's positions as machine words: one word for 64 positions or fewer, two
//! for 128, four or eight for 512, so the simulation's step is a few `or`s, `and`s and shifts
//! (and 64 for 4,096, only to build a DFA).

use std::array;

/// `1 << position` in a word, nothing for a position past it.
fn bit<T: From<u8> + std::ops::Shl<u32, Output = T>>(position: usize, width: u32) -> T {
    match u32::try_from(position) {
        Ok(shift) if shift < width => T::from(1) << shift,
        Ok(_) | Err(_) => T::from(0),
    }
}

/// A fixed-width set of positions.
pub(super) trait Bits: Copy + Ord {
    /// How many positions it holds (the tests' bound: an automaton is sized by `POSITIONS`).
    #[cfg(test)]
    const WIDTH: usize;
    fn none() -> Self;
    #[must_use]
    fn with(self, position: usize) -> Self;
    #[must_use]
    fn or(self, other: Self) -> Self;
    #[must_use]
    fn and(self, other: Self) -> Self;
    /// The positions of `self` not in `other`.
    #[must_use]
    fn minus(self, other: Self) -> Self;
    /// Each position moved to the next one, the last let go.
    #[must_use]
    fn shifted(self) -> Self;
    fn any(self) -> bool;
    /// The first position held.
    fn first(self) -> Option<usize>;
    /// Each position held, in order.
    fn each(self, f: impl FnMut(usize));
}

impl Bits for u64 {
    #[cfg(test)]
    const WIDTH: usize = 64;

    fn none() -> Self {
        0
    }

    fn with(self, position: usize) -> Self {
        self | bit::<Self>(position, Self::BITS)
    }

    fn minus(self, other: Self) -> Self {
        self & !other
    }

    fn shifted(self) -> Self {
        self << 1
    }

    fn or(self, other: Self) -> Self {
        self | other
    }

    fn and(self, other: Self) -> Self {
        self & other
    }

    fn any(self) -> bool {
        self != 0
    }

    fn first(self) -> Option<usize> {
        (self != 0).then(|| self.trailing_zeros() as usize)
    }

    fn each(self, mut f: impl FnMut(usize)) {
        let mut left = self;
        while left != 0 {
            f(left.trailing_zeros() as usize);
            left &= left - 1;
        }
    }
}

impl Bits for u128 {
    #[cfg(test)]
    const WIDTH: usize = 128;

    fn none() -> Self {
        0
    }

    fn with(self, position: usize) -> Self {
        self | bit::<Self>(position, Self::BITS)
    }

    fn minus(self, other: Self) -> Self {
        self & !other
    }

    fn shifted(self) -> Self {
        self << 1
    }

    fn or(self, other: Self) -> Self {
        self | other
    }

    fn and(self, other: Self) -> Self {
        self & other
    }

    fn any(self) -> bool {
        self != 0
    }

    fn first(self) -> Option<usize> {
        (self != 0).then(|| self.trailing_zeros() as usize)
    }

    fn each(self, mut f: impl FnMut(usize)) {
        let mut left = self;
        while left != 0 {
            f(left.trailing_zeros() as usize);
            left &= left - 1;
        }
    }
}

/// Several words, for patterns of more than 128 positions: `N` of them, `64 * N` positions.
impl<const N: usize> Bits for [u64; N] {
    #[cfg(test)]
    const WIDTH: usize = 64 * N;

    fn none() -> Self {
        [0; N]
    }

    fn with(mut self, position: usize) -> Self {
        if let Some(word) = self.get_mut(position / 64) {
            *word |= bit::<u64>(position % 64, 64);
        }
        self
    }

    fn minus(self, other: Self) -> Self {
        array::from_fn(|i| self[i] & !other[i])
    }

    fn shifted(self) -> Self {
        array::from_fn(|i| {
            let carry = i.checked_sub(1).map_or(0, |before| self[before] >> 63);
            self[i] << 1 | carry
        })
    }

    fn or(self, other: Self) -> Self {
        array::from_fn(|i| self[i] | other[i])
    }

    fn and(self, other: Self) -> Self {
        array::from_fn(|i| self[i] & other[i])
    }

    fn any(self) -> bool {
        self.iter().fold(0, |all, word| all | word) != 0
    }

    fn first(self) -> Option<usize> {
        let index = self.iter().position(|&word| word != 0)?;
        Some(index * 64 + self.get(index)?.trailing_zeros() as usize)
    }

    fn each(self, mut f: impl FnMut(usize)) {
        for (index, word) in self.into_iter().enumerate() {
            word.each(|bit| f(index * 64 + bit));
        }
    }
}

#[cfg(test)]
#[path = "bits_tests.rs"]
mod tests;
