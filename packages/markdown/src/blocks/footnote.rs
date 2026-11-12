//! A footnote's definition, `[^label]: text`, as GitHub reads it: a block that holds blocks, as
//! a list item does, its first line's text after the colon, and the lines after that are blank or
//! indented by four columns or more. It may interrupt a paragraph.

use super::{Blocks, Data, Go, Start};
use crate::notes::{label, stop};
use crate::scan::is_space;
use crate::tree::Kind;

impl Blocks<'_> {
    pub(super) fn footnote(&mut self) -> Start {
        let bytes = self.bytes;
        let line = bytes.get(..self.end).unwrap_or_default();
        if line.get(self.next + 1) != Some(&b'^') {
            return Start::None;
        }
        let start = self.next + 2;
        let end = stop(line, start);
        let Some(name) = label(line, start, end) else {
            return Start::None;
        };
        if line.get(end + 1) != Some(&b':') {
            return Start::None;
        }
        self.advance_next_nonspace();
        self.advance(end + 2 - self.next, false);
        // The spaces after the colon are the marker's: what follows is the definition's text.
        while is_space(self.byte(self.offset)) {
            self.advance(1, true);
        }
        self.close_unmatched();
        let node = self.add(Kind::Footnote);
        self.info(node).data = Data::Note;
        self.notes.define(name, node);
        Start::Container
    }

    /// Whether a line goes on with a footnote: blank, or indented by four columns.
    pub(super) fn note_continues(&mut self) -> Go {
        if self.blank {
            self.advance_next_nonspace();
        } else if self.indent >= 4 {
            self.advance(4, true);
        } else {
            return Go::Off;
        }
        Go::On
    }
}
