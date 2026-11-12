//! The blocks a line can start, tried in the order CommonMark gives them.

use super::{Blocks, Data, Start, html};
use crate::entity::{escaped, unescape};
use crate::scan::{is_space, trimmed};
use crate::tree::{Kind, Span, pos};

impl Blocks<'_> {
    pub(super) fn start(&mut self, container: u32) -> Start {
        if self.indent >= 4 {
            if matches!(self.kind(self.tip), Kind::Paragraph) {
                return Start::None;
            }
            self.advance(4, true);
            self.close_unmatched();
            let node = self.add(Kind::CodeBlock);
            self.info(node).data = Data::Indented;
            return Start::Leaf;
        }
        let paragraph = matches!(self.kind(container), Kind::Paragraph);
        match self.byte(self.next) {
            b'>' => {
                self.advance_next_nonspace();
                self.advance(1, false);
                if is_space(self.byte(self.offset)) {
                    self.advance(1, true);
                }
                self.close_unmatched();
                self.add(Kind::Quote);
                Start::Container
            }
            b'#' => self.heading(),
            b'`' | b'~' => self.fence(),
            b'<' => self.html(container),
            b'=' if paragraph => self.setext(container),
            b'-' => self
                .table(container)
                .or_else(|| self.setext(container))
                .or_else(|| self.rule())
                .or_else(|| self.item(container)),
            b'*' => self.rule().or_else(|| self.item(container)),
            b'_' => self.rule(),
            b'+' | b'0'..=b'9' => self.item(container),
            b'|' | b':' => self.table(container),
            _ => Start::None,
        }
    }

    fn heading(&mut self) -> Start {
        let rest = self.rest();
        let level = rest.iter().take_while(|byte| **byte == b'#').count();
        if level > 6 || rest.get(level).is_some_and(|byte| !is_space(*byte)) {
            return Start::None;
        }
        self.advance_next_nonspace();
        self.advance(level, false);
        self.close_unmatched();
        let node = self.add(Kind::Heading(u8::try_from(level).unwrap_or(6)));
        let (start, mut end) = trimmed(self.bytes, self.offset, self.end);
        // A closing run of `#`s is not part of the heading when a space comes before it.
        let mut close = end;
        while close > start && self.bytes[close - 1] == b'#' {
            close -= 1;
        }
        if close == start {
            end = start;
        } else if close < end && is_space(self.bytes[close - 1]) {
            end = trimmed(self.bytes, start, close).1;
        }
        self.doc.raw_mut(node).span = Span::new(pos(start), pos(end));
        self.info(node).filled = true;
        self.advance_to_end();
        Start::Leaf
    }

    fn fence(&mut self) -> Start {
        let rest = self.rest();
        let mark = rest[0];
        let len = rest.iter().take_while(|byte| **byte == mark).count();
        if len < 3 || (mark == b'`' && rest[len..].contains(&b'`')) {
            return Start::None;
        }
        let (start, end) = trimmed(self.bytes, self.next + len, self.end);
        let indent = self.indent;
        self.close_unmatched();
        let node = self.add(Kind::CodeBlock);
        self.info(node).data = Data::Fence {
            mark,
            len: pos(len),
            indent: pos(indent),
        };
        let info = self.src.get(start..end).unwrap_or_default();
        self.doc.raw_mut(node).extra = if escaped(info.as_bytes()) {
            let from = self.bytes.len() + self.doc.own.len();
            unescape(info, &mut self.doc.own);
            Span::new(pos(from), pos(self.bytes.len() + self.doc.own.len()))
        } else {
            Span::new(pos(start), pos(end))
        };
        self.advance_to_end();
        Start::Leaf
    }

    fn html(&mut self, container: u32) -> Start {
        let Some(kind) = html::opens(self.rest()) else {
            return Start::None;
        };
        // A lone tag does not interrupt a paragraph.
        let lazy = !self.all_closed() && matches!(self.kind(self.tip), Kind::Paragraph);
        if kind == 7 && (matches!(self.kind(container), Kind::Paragraph) || lazy) {
            return Start::None;
        }
        self.close_unmatched();
        let node = self.add(Kind::HtmlBlock);
        self.info(node).data = Data::Html(kind);
        Start::Leaf
    }

    /// A line of `=` or `-` under a paragraph makes it a heading.
    fn setext(&mut self, container: u32) -> Start {
        let rest = self.rest();
        let mark = rest[0];
        let run = rest.iter().take_while(|byte| **byte == mark).count();
        if !matches!(self.kind(container), Kind::Paragraph)
            || !rest[run..].iter().all(|byte| is_space(*byte))
        {
            return Start::None;
        }
        self.close_unmatched();
        if !self.definitions(container) {
            return Start::None;
        }
        let level = if mark == b'=' { 1 } else { 2 };
        self.doc.raw_mut(container).kind = Kind::Heading(level);
        self.advance_to_end();
        Start::Leaf
    }

    fn rule(&mut self) -> Start {
        let rest = self.rest();
        let mark = rest[0];
        // One pass with no early way out, so the bytes are compared many at a time.
        let (marks, others) = rest.iter().fold((0, 0), |(marks, others), byte| {
            let marked = *byte == mark;
            let other = !marked & (*byte != b' ') & (*byte != b'\t');
            (marks + usize::from(marked), others + usize::from(other))
        });
        if marks < 3 || others > 0 {
            return Start::None;
        }
        self.close_unmatched();
        self.add(Kind::Rule);
        self.advance_to_end();
        Start::Leaf
    }
}

impl Start {
    /// This start, or the next one tried when the line did not start it.
    pub(super) fn or_else(self, next: impl FnOnce() -> Self) -> Self {
        match self {
            Self::None => next(),
            Self::Container | Self::Leaf => self,
        }
    }
}
