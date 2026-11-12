//! A reference to a footnote, `[^label]`: one only when the document defines that label, else
//! the bracket it starts with.

use super::Inline;
use crate::notes::{label, stop};
use crate::tree::Kind;

impl Inline<'_, '_, '_> {
    /// At a `[` before a `^`: a reference, or a bracket.
    pub(super) fn note(&mut self) {
        let Some((end, number)) = self.footnote_ref() else {
            return self.open(1);
        };
        self.flush(self.pos);
        let span = self.span(self.pos + 2, end);
        let node = self.push(Kind::FootnoteRef, span);
        self.doc.raw_mut(node).num = number;
        self.moved(end + 1);
    }

    /// Whether a `^` is at `at` where the document defines footnotes: a `!` before its `[` is no
    /// image's.
    pub(super) fn noted(&self, at: usize) -> bool {
        !self.notes.is_empty() && self.bytes.get(at) == Some(&b'^')
    }

    /// The reference at the `[`: where its `]` is, and the footnote's number.
    fn footnote_ref(&mut self) -> Option<(usize, u32)> {
        if self.notes.is_empty() {
            return None;
        }
        let start = self.pos + 2;
        // Where the last label looked for stopped holds for every one that starts before it: a
        // text of `[^` alone is read once.
        if self.note_stop < start {
            self.note_stop = stop(self.bytes, start);
        }
        let end = self.note_stop;
        let number = self.notes.refer(label(self.bytes, start, end)?)?;
        Some((end, number))
    }
}
