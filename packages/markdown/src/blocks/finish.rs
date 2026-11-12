//! A block closed: what is known of it only once its last line is in.

use super::{Blocks, Data};
use crate::scan::is_blank;
use crate::tree::{Kind, NONE, at, pos};

impl Blocks<'_> {
    pub(super) fn finalize(&mut self, node: u32, line: u32) {
        let above = self.doc.raw(node).parent;
        let kind = self.kind(node);
        let last = self.doc.raw(node).last;
        let info = self.info(node);
        info.open = false;
        info.end_line = line;
        let data = info.data;
        if matches!(kind, Kind::Paragraph) {
            if !self.definitions(node) {
                self.doc.unlink(node);
            }
        } else if matches!(data, Data::Indented) {
            self.trim_code(node);
        } else if let Data::Fence { mark, .. } = data {
            self.formula(node, mark);
        } else if let Kind::List { ordered, .. } = kind {
            let tight = self.tight(node);
            self.doc.raw_mut(node).kind = Kind::List { ordered, tight };
            if last != NONE {
                self.info(node).end_line = self.info[at(last)].end_line;
            }
        } else if matches!(kind, Kind::Item(_)) {
            let start = self.info[at(node)].start_line;
            self.info(node).end_line = if last == NONE {
                start
            } else {
                self.info[at(last)].end_line
            };
        }
        self.tip = if above == NONE { 0 } else { above };
    }

    /// The definitions a paragraph starts with, taken out of it: whether any text is left.
    pub(super) fn definitions(&mut self, node: u32) -> bool {
        loop {
            let span = self.doc.raw(node).span;
            let text = self.doc.text(span);
            if !text.starts_with('[') {
                let left = text.bytes().any(|byte| !is_blank(byte));
                if !left {
                    self.info(node).filled = false;
                }
                return left;
            }
            // A definition has its colon right after its label: most bracketed text has none.
            if !text.contains("]:") {
                return true;
            }
            let text = text.to_owned();
            let edge = self.bytes.len();
            match self.refs.define(&text, &mut self.doc.own, edge) {
                Some(taken) => self.doc.raw_mut(node).span.start += pos(taken),
                None => return true,
            }
        }
    }

    /// A fence of dollars, or one whose language is `math`, is a formula where formulas are read:
    /// its text the LaTeX, with no line ending after it.
    fn formula(&mut self, node: u32, mark: u8) {
        if !self.reads.math {
            return;
        }
        let raw = self.doc.raw(node);
        let (span, extra) = (raw.span, raw.extra);
        let math =
            mark == b'$' || self.doc.text(extra).split_ascii_whitespace().next() == Some("math");
        if !math {
            return;
        }
        let end = span.end - u32::from(self.doc.text(span).ends_with('\n'));
        let raw = self.doc.raw_mut(node);
        raw.kind = Kind::MathBlock;
        raw.span.end = end;
    }

    /// An indented code block ends at its last line that is not blank.
    fn trim_code(&mut self, node: u32) {
        let span = self.doc.raw(node).span;
        let text = self.doc.text(span).as_bytes();
        let mut end = text.len();
        while end > 0 {
            let line = text[..end - 1]
                .iter()
                .rposition(|byte| *byte == b'\n')
                .map_or(0, |at| at + 1);
            if text[line..end].iter().any(|byte| !is_blank(*byte)) {
                break;
            }
            end = line;
        }
        self.doc.raw_mut(node).span.end = span.start + pos(end);
    }

    /// Whether a blank line parts `first` from the sibling after it.
    fn parted(&self, first: u32) -> bool {
        let next = self.doc.raw(first).next;
        next != NONE && self.info[at(first)].end_line + 1 != self.info[at(next)].start_line
    }

    /// A list is tight when no blank line parts its items, nor the blocks of any of them.
    fn tight(&self, list: u32) -> bool {
        let mut item = self.doc.raw(list).first;
        while item != NONE {
            if self.parted(item) {
                return false;
            }
            let mut block = self.doc.raw(item).first;
            while block != NONE {
                if self.parted(block) {
                    return false;
                }
                block = self.doc.raw(block).next;
            }
            item = self.doc.raw(item).next;
        }
        true
    }
}
