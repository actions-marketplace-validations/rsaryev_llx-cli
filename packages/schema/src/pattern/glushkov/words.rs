//! `\b` and `\B` as the DFA takes them: whether one holds between two characters is known only
//! once both are read, so a DFA state keeps whether the character before was a word character
//! (the text's start and end count as not one), and each step resolves the paths through the
//! markers with the character it reads: those that hold enter their followers, or end a match
//! found before it. Word characters are ECMA-262's `\w`, ASCII's letters, digits and `_`.

use super::super::bits::Bits;
use super::anchors::Words;
use super::jumps::Jumps;

/// Whether `c` is a word character.
pub(super) fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The paths through `\b` (index 0 of each pair) and `\B` (index 1).
pub(super) struct Bounds<B> {
    /// Where a match starts after them: anywhere, or at the text's start.
    first: [[B; 2]; 2],
    /// The positions a match ends at before them: anywhere, or at the text's end.
    last: [[B; 2]; 2],
    /// An empty match through them alone, by whether it asked the start, the end.
    empty: [[[bool; 2]; 2]; 2],
    /// The followers through them.
    jumps: [Jumps<B>; 2],
}

/// The index of what holds between two characters, by whether one of them is a word character
/// and the other not: `\b`'s or `\B`'s.
fn side(bound: bool) -> usize {
    usize::from(!bound)
}

impl<B: Bits> Bounds<B> {
    pub(super) fn new(words: Words<B>) -> Self {
        let [bound, inside] = words.follow;
        Self {
            first: words.first,
            last: words.last,
            empty: words.empty,
            jumps: [Jumps::new(&bound), Jumps::new(&inside)],
        }
    }

    /// The positions entered through them after `set`, `bound` saying whether the characters
    /// on either side differ (from the text's start when `start`).
    pub(super) fn entered(&self, set: B, start: bool, bound: bool) -> B {
        let side = side(bound);
        let mut to = self.jumps[side].of(set).or(self.first[0][side]);
        if start {
            to = to.or(self.first[1][side]);
        }
        to
    }

    /// Whether a match ends between the characters, after `set`.
    pub(super) fn ended(&self, set: B, start: bool, bound: bool) -> bool {
        let side = side(bound);
        set.and(self.last[0][side]).any()
            || self.empty[0][0][side]
            || (start && self.empty[1][0][side])
    }

    /// Whether a match ends at the end of a text that is not empty, after `set`, its last
    /// character a word character when `word`.
    pub(super) fn at_end(&self, set: B, word: bool) -> bool {
        let side = side(word);
        set.and(self.last[0][side].or(self.last[1][side])).any()
            || self.empty[0][0][side]
            || self.empty[0][1][side]
    }

    /// Whether the empty text matches through them: no boundary is there.
    pub(super) fn empty_text(&self) -> bool {
        self.empty.iter().flatten().any(|empty| empty[1])
    }

    /// Whether a match may still start where no position is active.
    pub(super) fn hopes(&self) -> bool {
        self.first[0].iter().any(|&first| first.any())
            || self.empty[0].iter().flatten().any(|&empty| empty)
    }
}
