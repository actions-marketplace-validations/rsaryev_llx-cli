//! The followers a position has besides the next one and itself (a group's start after its end,
//! what comes after an alternative), kept as sets: each set once, with the positions it follows,
//! so a step costs one `or` for each set reached, however many positions reach it
//! (`[a-z]{1,4000}x`: four thousand positions, one set, `x`).

use std::collections::BTreeMap;

use super::super::bits::Bits;

pub(super) struct Jumps<B> {
    /// The positions with such followers.
    from: B,
    /// Each set of followers, with the positions it follows.
    sets: Vec<(B, B)>,
    /// Each position's set, by its index in `sets` (none for a position that has none).
    of: Vec<usize>,
}

impl<B: Bits> Jumps<B> {
    /// From each position's followers, those of the next position and itself already taken out.
    pub(super) fn new(rests: &[B]) -> Self {
        let mut grouped: BTreeMap<B, B> = BTreeMap::new();
        let mut from = B::none();
        for (position, &rest) in rests.iter().enumerate() {
            if rest.any() {
                from = from.with(position);
                let positions = grouped.entry(rest).or_insert_with(B::none);
                *positions = positions.with(position);
            }
        }
        let mut of = vec![usize::MAX; rests.len()];
        let mut sets = Vec::with_capacity(grouped.len());
        for (index, (followers, positions)) in grouped.into_iter().enumerate() {
            positions.each(|position| {
                if let Some(slot) = of.get_mut(position) {
                    *slot = index;
                }
            });
            sets.push((positions, followers));
        }
        Self { from, sets, of }
    }

    /// Whether any position has such followers.
    pub(super) fn any(&self) -> bool {
        self.from.any()
    }

    /// The followers of the positions of `set`.
    pub(super) fn of(&self, set: B) -> B {
        let mut to = B::none();
        let mut left = set.and(self.from);
        while let Some(position) = left.first() {
            left = left.minus(B::none().with(position));
            let jump = self.of.get(position).and_then(|&at| self.sets.get(at));
            if let Some(&(positions, followers)) = jump {
                to = to.or(followers);
                left = left.minus(positions);
            }
        }
        to
    }
}

#[cfg(test)]
#[path = "jumps_tests.rs"]
mod tests;
