//! Where the block pass is in its line: an offset in the source and the column it stands at, a
//! tab being up to four columns wide.

use super::Blocks;
use crate::scan::is_space;
use crate::tree::{Kind, Span, at, pos};

impl<'a> Blocks<'a> {
    /// The byte at `at`, or 0 past the line's end. A NUL in the text itself is no marker of any
    /// block, so it reads as the text it is.
    #[inline(always)]
    pub(super) fn byte(&self, at: usize) -> u8 {
        if at < self.end { self.bytes[at] } else { 0 }
    }

    /// The line from its first character that is not a space, to its end.
    pub(super) fn rest(&self) -> &'a [u8] {
        &self.bytes[self.next.min(self.end)..self.end]
    }

    pub(super) fn find_next_nonspace(&mut self) {
        let mut next = self.offset;
        let mut column = self.column;
        loop {
            match self.byte(next) {
                b' ' => column += 1,
                b'\t' => column += 4 - column % 4,
                _ => break,
            }
            next += 1;
        }
        self.blank = next >= self.end;
        self.next = next;
        self.next_column = column;
        self.indent = column - self.column;
    }

    pub(super) fn advance_next_nonspace(&mut self) {
        self.offset = self.next;
        self.column = self.next_column;
        self.partial_tab = false;
    }

    /// Moves past `count` characters, or that many columns.
    pub(super) fn advance(&mut self, count: usize, columns: bool) {
        let mut left = count;
        while left > 0 && self.offset < self.end {
            if self.bytes[self.offset] == b'\t' {
                let to_tab = 4 - self.column % 4;
                if columns {
                    self.partial_tab = to_tab > left;
                    let step = to_tab.min(left);
                    self.column += step;
                    if !self.partial_tab {
                        self.offset += 1;
                    }
                    left -= step;
                } else {
                    self.partial_tab = false;
                    self.column += to_tab;
                    self.offset += 1;
                    left -= 1;
                }
            } else {
                self.partial_tab = false;
                self.offset += 1;
                self.column += 1;
                left -= 1;
            }
        }
    }

    pub(super) fn advance_to_end(&mut self) {
        self.offset = self.end;
        self.partial_tab = false;
    }

    /// The rest of the line added to the tip, with its line ending. The text stays a cut of the
    /// source while its lines follow each other there; a line that does not (a quote's `>` came
    /// before it, a tab was half taken) moves the text into the document's own.
    pub(super) fn add_line(&mut self) {
        let tip = self.tip;
        let mut pad = 0;
        if self.partial_tab {
            // The rest of a tab that a container's indent took part of.
            self.offset += 1;
            pad = 4 - self.column % 4;
        }
        let (start, end) = (self.offset.min(self.end), self.end);
        let span = self.doc.raw(tip).span;
        let filled = self.info[at(tip)].filled;
        // The text's last line may have no ending: a paragraph and an HTML block drop theirs
        // anyway, any other block is given one.
        let ending = match self.bytes.get(end) {
            Some(b'\n') => Some(1),
            None if matches!(self.kind(tip), Kind::Paragraph | Kind::HtmlBlock) => Some(0),
            Some(_) | None => None,
        };
        if let Some(ending) = ending.filter(|_| pad == 0) {
            if !filled {
                self.doc.raw_mut(tip).span = Span::new(pos(start), pos(end + ending));
                self.info(tip).filled = true;
                return;
            }
            if at(span.start) < self.bytes.len() && self.joins(tip, at(span.end), start) {
                self.doc.raw_mut(tip).span.end = pos(end + ending);
                return;
            }
        }
        self.own(tip);
        let own = &mut self.doc.own;
        own.extend(std::iter::repeat_n(' ', pad));
        own.push_str(self.src.get(start..end).unwrap_or_default());
        own.push('\n');
        self.doc.raw_mut(tip).span.end = pos(self.bytes.len() + self.doc.own.len());
    }

    /// Whether a line at `start` goes on a text that ends at `end` in the source: right after it,
    /// or after an indent a paragraph drops anyway.
    fn joins(&self, node: u32, end: usize, start: usize) -> bool {
        end == start
            || (end < start
                && matches!(self.kind(node), Kind::Paragraph)
                && self.bytes[end..start].iter().all(|byte| is_space(*byte)))
    }

    /// The text of `node` moved to the end of the document's own text, where it can grow.
    pub(super) fn own(&mut self, node: u32) {
        self.doc.room();
        let edge = self.bytes.len();
        let tail = edge + self.doc.own.len();
        let span = self.doc.raw(node).span;
        let filled = std::mem::replace(&mut self.info(node).filled, true);
        if filled && at(span.end) == tail && at(span.start) >= edge {
            return;
        }
        if filled {
            let (start, end) = (at(span.start), at(span.end));
            if start < edge {
                let text = self.doc.src.get(start..end).unwrap_or_default();
                self.doc.own.push_str(text);
            } else {
                self.doc.own.extend_from_within(start - edge..end - edge);
            }
        }
        let end = edge + self.doc.own.len();
        self.doc.raw_mut(node).span = Span::new(pos(tail), pos(end));
    }
}
