//! A document as its reader walks it: a node, its kind, its texts, its children.

use crate::tree::{Align, Document, Kind, NONE, Raw, at};

impl Document<'_> {
    /// The document itself, whose children are its blocks.
    #[must_use]
    pub fn root(&self) -> Node<'_> {
        Node { doc: self, id: 0 }
    }

    /// The top-level blocks, in order.
    #[must_use]
    pub fn blocks(&self) -> Children<'_> {
        self.root().children()
    }
}

/// A node of a document.
#[derive(Clone, Copy, Debug)]
pub struct Node<'d> {
    doc: &'d Document<'d>,
    id: u32,
}

impl<'d> Node<'d> {
    fn raw(self) -> &'d Raw {
        self.doc.raw(self.id)
    }

    #[must_use]
    #[inline]
    pub fn kind(self) -> Kind {
        self.raw().kind
    }

    /// The node's own text: a text's, a code's, a block of HTML's, a formula's LaTeX. Empty for
    /// the rest.
    #[must_use]
    #[inline]
    pub fn text(self) -> &'d str {
        let raw = self.raw();
        let own = matches!(
            raw.kind,
            Kind::Text
                | Kind::Code
                | Kind::CodeBlock
                | Kind::Html
                | Kind::HtmlBlock
                | Kind::Math
                | Kind::MathBlock
        );
        if own { self.doc.text(raw.span) } else { "" }
    }

    /// What follows a code block's fence: its language, then anything else on the line.
    #[must_use]
    pub fn info(self) -> &'d str {
        let raw = self.raw();
        if matches!(raw.kind, Kind::CodeBlock) {
            self.doc.text(raw.extra)
        } else {
            ""
        }
    }

    /// A link's or an image's destination.
    #[must_use]
    pub fn dest(self) -> &'d str {
        let raw = self.raw();
        if matches!(raw.kind, Kind::Link | Kind::Image) {
            self.doc.text(raw.extra)
        } else {
            ""
        }
    }

    /// A link's or an image's title, a form's (the text after its marker), a tab's label.
    #[must_use]
    pub fn title(self) -> &'d str {
        let raw = self.raw();
        if matches!(raw.kind, Kind::Link | Kind::Image) {
            let title = raw.num.checked_sub(1);
            let span = title.and_then(|place| self.doc.titles.get(at(place)));
            span.map_or("", |span| self.doc.text(*span))
        } else if raw.kind.is_form() {
            self.doc.text(raw.extra)
        } else {
            ""
        }
    }

    /// An ordered list's first number; a footnote's, and a reference's to it.
    #[must_use]
    pub fn start(self) -> u32 {
        self.raw().num
    }

    /// A table's alignments, one a column.
    #[must_use]
    pub fn aligns(self) -> &'d [Align] {
        let raw = self.raw();
        if !matches!(raw.kind, Kind::Table) {
            return &[];
        }
        let first = at(raw.num);
        let aligns = self.doc.aligns.get(first..first + at(raw.extra.start));
        aligns.unwrap_or_default()
    }

    #[must_use]
    #[inline]
    pub fn children(self) -> Children<'d> {
        Children {
            doc: self.doc,
            next: self.raw().first,
        }
    }

    #[must_use]
    #[inline]
    pub fn first(self) -> Option<Self> {
        self.children().next()
    }

    /// The sibling after this node.
    #[must_use]
    #[inline]
    pub fn next(self) -> Option<Self> {
        let next = self.raw().next;
        (next != NONE).then_some(Node {
            doc: self.doc,
            id: next,
        })
    }
}

/// A node's children, in order.
#[derive(Clone, Copy, Debug)]
pub struct Children<'d> {
    doc: &'d Document<'d>,
    next: u32,
}

impl<'d> Iterator for Children<'d> {
    type Item = Node<'d>;

    fn next(&mut self) -> Option<Node<'d>> {
        if self.next == NONE {
            return None;
        }
        let node = Node {
            doc: self.doc,
            id: self.next,
        };
        self.next = self.doc.raw(self.next).next;
        Some(node)
    }
}
