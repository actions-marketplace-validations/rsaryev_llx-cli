//! The tree a text is read into: every node in one arena, each naming its place in the text it
//! was read from. A node's text is a cut of the source wherever the source holds it as it is
//! shown, so reading a document copies only what had to change (an entity, a line inside a quote).

use std::borrow::Cow;

/// No node: the end of a list of siblings, a node with no children.
pub(crate) const NONE: u32 = u32::MAX;

/// A cut of the document's text: the source first, then what the parser wrote itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Span {
    pub(crate) start: u32,
    pub(crate) end: u32,
}

impl Span {
    pub(crate) const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

/// A position as the tree keeps it. A text past 4 GiB is cut before it is read (`parse`), so
/// every position fits.
#[inline(always)]
pub(crate) fn pos(at: usize) -> u32 {
    u32::try_from(at).unwrap_or(u32::MAX)
}

/// A position as a slice takes it.
#[inline(always)]
pub(crate) const fn at(pos: u32) -> usize {
    pos as usize
}

/// A text has about a block for this many of its bytes: what the block pass makes room for.
pub(crate) const BLOCK: usize = 48;
/// And a block's text an inline for this many of its bytes, when it is full of marks.
pub(crate) const INLINE: usize = 8;
/// A text shorter than this is given the room for all its nodes at once.
pub(crate) const SHORT: usize = 4096;

/// How a table's column is aligned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

/// Whether a list item is a task, and whether it is done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Task {
    None,
    Todo,
    Done,
}

/// A callout's kind, as GitHub's alerts name them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callout {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

/// What a node is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Document,
    Paragraph,
    /// Its level, 1 to 6.
    Heading(u8),
    Rule,
    Quote,
    List {
        ordered: bool,
        /// No blank line parts its items.
        tight: bool,
    },
    Item(Task),
    /// `text` is the code, `info` what follows the fence.
    CodeBlock,
    HtmlBlock,
    Table,
    Row {
        head: bool,
    },
    Cell(Align),
    Text,
    SoftBreak,
    HardBreak,
    Emphasis,
    Strong,
    Strike,
    /// `text` is the code.
    Code,
    /// `dest` and `title`; its children are the link's text.
    Link,
    /// `dest` and `title`; its children are the description.
    Image,
    Html,
    /// The forms of an agent's reply; `title` is the text after the marker.
    Callout(Callout),
    /// Stages in order: its one child is their list.
    Steps,
    /// Variants side by side: its children are `Tab`s.
    Tabs,
    /// One variant: `title` is its label, its children what it holds.
    Tab,
    /// Headline figures: its one child is their list.
    Stats,
    Pros,
    Cons,
    /// Secondary material under its title.
    Details,
    /// The pages a reply draws on: its one child is their list.
    Sources,
}

impl Kind {
    /// Whether it is one of the forms of an agent's reply, or a tab of one.
    #[must_use]
    pub const fn is_form(self) -> bool {
        matches!(
            self,
            Self::Callout(_)
                | Self::Steps
                | Self::Tabs
                | Self::Tab
                | Self::Stats
                | Self::Pros
                | Self::Cons
                | Self::Details
                | Self::Sources
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Raw {
    pub(crate) kind: Kind,
    /// The node's own text: a paragraph's source before its inlines are read, a text's, a code's.
    pub(crate) span: Span,
    /// A second text: a fence's info, a link's destination, a form's title.
    pub(crate) extra: Span,
    /// A number: a list's first, a link's title (one more than its place in `titles`), a table's
    /// first alignment.
    pub(crate) num: u32,
    pub(crate) parent: u32,
    pub(crate) prev: u32,
    pub(crate) next: u32,
    pub(crate) first: u32,
    pub(crate) last: u32,
}

/// A document read: its nodes and the text they are cut from.
#[derive(Debug)]
pub struct Document<'a> {
    pub(crate) src: Cow<'a, str>,
    /// What the parser wrote itself, addressed right after the source.
    pub(crate) own: String,
    pub(crate) nodes: Vec<Raw>,
    pub(crate) titles: Vec<Span>,
    pub(crate) aligns: Vec<Align>,
}

impl<'a> Document<'a> {
    pub(crate) fn new(src: &'a str) -> Self {
        let room = if src.len() < SHORT { INLINE } else { BLOCK };
        let mut nodes = Vec::with_capacity(src.len() / room + 32);
        nodes.push(Raw {
            kind: Kind::Document,
            span: Span::default(),
            extra: Span::default(),
            num: 0,
            parent: NONE,
            prev: NONE,
            next: NONE,
            first: NONE,
            last: NONE,
        });
        Self {
            src: Cow::Borrowed(src),
            own: String::new(),
            nodes,
            titles: Vec::new(),
            aligns: Vec::new(),
        }
    }

    /// The document with its text its own, for a caller that cannot keep the source.
    #[must_use]
    pub fn into_owned(self) -> Document<'static> {
        Document {
            src: Cow::Owned(self.src.into_owned()),
            own: self.own,
            nodes: self.nodes,
            titles: self.titles,
            aligns: self.aligns,
        }
    }

    pub(crate) fn text(&self, span: Span) -> &str {
        let edge = self.src.len();
        let (start, end) = (at(span.start), at(span.end));
        let cut = if start < edge {
            self.src.get(start..end)
        } else {
            self.own.get(start - edge..end.saturating_sub(edge))
        };
        cut.unwrap_or_default()
    }

    #[inline(always)]
    pub(crate) fn raw(&self, id: u32) -> &Raw {
        &self.nodes[at(id)]
    }

    #[inline(always)]
    pub(crate) fn raw_mut(&mut self, id: u32) -> &mut Raw {
        &mut self.nodes[at(id)]
    }

    /// Room in the document's own text before it is first written to: it would grow a few bytes
    /// at a time.
    #[inline(always)]
    pub(crate) fn room(&mut self) {
        if self.own.capacity() == 0 {
            self.own.reserve(self.src.len() / 16 + 64);
        }
    }

    /// A node of `kind` with no place in the tree yet.
    #[inline(always)]
    pub(crate) fn make(&mut self, kind: Kind, span: Span) -> u32 {
        let id = pos(self.nodes.len());
        self.nodes.push(Raw {
            kind,
            span,
            extra: Span::default(),
            num: 0,
            parent: NONE,
            prev: NONE,
            next: NONE,
            first: NONE,
            last: NONE,
        });
        id
    }

    /// `child` as the last child of `parent`.
    #[inline(always)]
    pub(crate) fn append(&mut self, parent: u32, child: u32) {
        let last = self.raw(parent).last;
        {
            let node = self.raw_mut(child);
            node.parent = parent;
            node.prev = last;
            node.next = NONE;
        }
        if last == NONE {
            self.raw_mut(parent).first = child;
        } else {
            self.raw_mut(last).next = child;
        }
        self.raw_mut(parent).last = child;
    }

    /// A new last child of `parent`.
    #[inline(always)]
    pub(crate) fn push(&mut self, parent: u32, kind: Kind, span: Span) -> u32 {
        let id = self.make(kind, span);
        self.append(parent, id);
        id
    }

    /// `node` taken out of the tree, its children with it.
    pub(crate) fn unlink(&mut self, node: u32) {
        let Raw {
            parent, prev, next, ..
        } = *self.raw(node);
        if prev == NONE {
            if parent != NONE {
                self.raw_mut(parent).first = next;
            }
        } else {
            self.raw_mut(prev).next = next;
        }
        if next == NONE {
            if parent != NONE {
                self.raw_mut(parent).last = prev;
            }
        } else {
            self.raw_mut(next).prev = prev;
        }
        let raw = self.raw_mut(node);
        raw.parent = NONE;
        raw.prev = NONE;
        raw.next = NONE;
    }

    /// `node` placed right after `sibling`, under its parent.
    pub(crate) fn insert_after(&mut self, sibling: u32, node: u32) {
        let Raw { parent, next, .. } = *self.raw(sibling);
        {
            let raw = self.raw_mut(node);
            raw.parent = parent;
            raw.prev = sibling;
            raw.next = next;
        }
        self.raw_mut(sibling).next = node;
        if next == NONE {
            if parent != NONE {
                self.raw_mut(parent).last = node;
            }
        } else {
            self.raw_mut(next).prev = node;
        }
    }

    /// Every sibling after `from`, up to `until` (left where it is; `NONE` for all of them),
    /// moved under `into` in their order.
    pub(crate) fn adopt(&mut self, into: u32, from: u32, until: u32) {
        let mut child = self.raw(from).next;
        while child != NONE && child != until {
            let next = self.raw(child).next;
            self.unlink(child);
            self.append(into, child);
            child = next;
        }
    }
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
