//! Links and images: each `[` kept on a stack until a `]` shows what it opened, a link written in
//! place (`[text](destination "title")`), one that names a definition, or nothing.

use super::Inline;
use crate::link::{self, spnl};
use crate::tree::{Kind, NONE, Span, pos};

#[derive(Clone, Copy)]
pub(super) struct Bracket {
    /// The text node of the `[` or `![`.
    node: u32,
    image: bool,
    /// The delimiter on top of the stack as the bracket was read.
    delim: usize,
    /// Where the text after the bracket starts.
    at: usize,
    /// Whether another bracket was opened after this one.
    followed: bool,
}

/// The brackets open, and which of them may still open a link: a link's text holds no other
/// link, so every `[` before one is spent. Counted, never walked: text may open thousands.
#[derive(Default)]
pub(super) struct Brackets {
    open: Vec<Bracket>,
    /// The `[` below this place in the stack are spent.
    spent: usize,
    /// How many `[` above it there are.
    live: usize,
}

impl Brackets {
    pub(super) fn clear(&mut self) {
        self.open.clear();
        self.spent = 0;
        self.live = 0;
    }

    /// Whether the text being read is inside a `[` that may still open a link.
    pub(super) const fn links(&self) -> bool {
        self.live > 0
    }

    fn push(&mut self, bracket: Bracket) {
        if let Some(last) = self.open.last_mut() {
            last.followed = true;
        }
        self.live += usize::from(!bracket.image);
        if self.open.capacity() == 0 {
            self.open.reserve(super::STACK);
        }
        self.open.push(bracket);
    }

    /// The last bracket opened, and whether it may open a link or an image.
    fn pop(&mut self) -> Option<(Bracket, bool)> {
        let bracket = self.open.pop()?;
        let active = bracket.image || self.open.len() >= self.spent;
        self.live -= usize::from(active && !bracket.image);
        self.spent = self.spent.min(self.open.len());
        Some((bracket, active))
    }

    fn spend(&mut self) {
        self.spent = self.open.len();
        self.live = 0;
    }
}

impl Inline<'_, '_, '_> {
    /// A `[`, or the `![` of an image: `len` bytes.
    pub(super) fn open(&mut self, len: usize) {
        self.flush(self.pos);
        let span = self.span(self.pos, self.pos + len);
        let node = self.push(Kind::Text, span);
        self.apart = true;
        self.moved(self.pos + len);
        self.stacks.brackets.push(Bracket {
            node,
            image: len == 2,
            delim: self
                .stacks
                .delims
                .len()
                .checked_sub(1)
                .unwrap_or(usize::MAX),
            at: self.pos,
            followed: false,
        });
    }

    /// `(destination "title")` from `at`, its `(`: the two, and where it ends.
    fn inline(&mut self, at: usize) -> Option<(Span, Option<Span>, usize)> {
        let dest = link::destination(self.bytes, spnl(self.bytes, at + 1))?;
        let spaced = spnl(self.bytes, dest.after);
        let title = (spaced > dest.after)
            .then(|| link::title(self.bytes, spaced))
            .flatten();
        let end = spnl(self.bytes, title.map_or(spaced, |title| title.after));
        if self.bytes.get(end) != Some(&b')') {
            return None;
        }
        let dest = self.decoded(dest.start, dest.end);
        let title = title.map(|title| self.decoded(title.start, title.end));
        Some((dest, title, end + 1))
    }

    /// The definition a `]` at `start` names: by the label after it, else by the bracket's text.
    fn named(&self, opener: &Bracket, start: usize) -> Option<(Span, Option<Span>, usize)> {
        if self.refs.is_empty() {
            return None;
        }
        let label = link::label(self.bytes, start + 1);
        let key = match label {
            Some(end) if end - start > 3 => self.text.get(start + 2..end - 1),
            Some(_) | None if !opener.followed => self.text.get(opener.at..start),
            Some(_) | None => None,
        };
        let def = self.refs.get(key?)?;
        Some((def.dest, def.title, label.unwrap_or(start + 1)))
    }

    pub(super) fn close(&mut self) {
        let start = self.pos;
        let Some((opener, active)) = self.stacks.brackets.pop() else {
            self.pos += 1;
            return;
        };
        let found = if active {
            let inline = (self.bytes.get(start + 1) == Some(&b'('))
                .then(|| self.inline(start + 1))
                .flatten();
            inline.or_else(|| self.named(&opener, start))
        } else {
            None
        };
        let Some((dest, title, end)) = found else {
            self.pos += 1;
            return;
        };
        self.flush(start);
        let kind = if opener.image {
            Kind::Image
        } else {
            Kind::Link
        };
        let node = self.doc.make(kind, Span::default());
        self.doc.raw_mut(node).extra = dest;
        if let Some(title) = title {
            self.doc.titles.push(title);
            self.doc.raw_mut(node).num = pos(self.doc.titles.len());
        }
        self.doc.adopt(node, opener.node, NONE);
        self.doc.insert_after(opener.node, node);
        self.doc.unlink(opener.node);
        self.process(opener.delim);
        if !opener.image {
            self.stacks.brackets.spend();
        }
        self.moved(end);
    }
}
