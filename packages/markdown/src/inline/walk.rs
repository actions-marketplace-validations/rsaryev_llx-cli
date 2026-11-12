//! Every block read into its inlines, in the order the blocks were read: a paragraph's and a
//! heading's text, a table row's cells.

use super::{Inline, Pace, SLACK, Stacks};
use crate::blocks::table::cells;
use crate::forms;
use crate::refs::Refs;
use crate::scan::{is_blank, trimmed};
use crate::tree::{Document, INLINE, Kind, NONE, Raw, SHORT, Span, Task, at, pos};

/// Of a text not short, this part is given room first: an eighth.
const PART: usize = 8;

/// What is read beyond CommonMark.
#[derive(Clone, Copy)]
pub(crate) struct Reads {
    pub(crate) gfm: bool,
    pub(crate) forms: bool,
}

struct Walk<'s, 'd, 'a> {
    doc: &'d mut Document<'a>,
    refs: &'d Refs,
    stacks: Stacks,
    src: &'s str,
    /// What the block pass wrote, read here while this pass writes its own after it.
    old: &'s str,
    reads: Reads,
    /// How many cells a row short of its table's columns may still be filled with: as many as
    /// the text has bytes, or a table of a thousand columns makes a million from a thousand rows.
    spare: usize,
    pace: Pace,
}

pub(crate) fn read(doc: &mut Document<'_>, src: &str, refs: &Refs, reads: Reads) {
    let old = std::mem::take(&mut doc.own);
    let blocks = doc.nodes.len();
    let mut walk = Walk {
        doc,
        refs,
        stacks: Stacks::new(),
        src,
        old: &old,
        reads,
        spare: src.len(),
        pace: Pace {
            blocks,
            total: 0,
            read: 0,
        },
    };
    // Room for the inlines: a short text's at once, a long one's for its first part; then each
    // time little is left, for the rest by how many the text read so far took (`Inline::fit`).
    let inline = |raw: &Raw| {
        raw.parent != NONE
            && matches!(
                raw.kind,
                Kind::Paragraph | Kind::Heading(_) | Kind::Row { .. }
            )
    };
    let length = |raw: &Raw| at(raw.span.end.saturating_sub(raw.span.start));
    let texts = walk.doc.nodes.iter().filter(|raw| inline(raw));
    walk.pace.total = texts.map(length).sum();
    let total = walk.pace.total;
    let first = if total < SHORT { total } else { total / PART };
    walk.doc.nodes.reserve(first / INLINE + SLACK);
    let mut tabs = Vec::new();
    for id in 0..blocks {
        let node = pos(id);
        let raw = walk.doc.nodes[id];
        if !inline(&raw) {
            continue;
        }
        if matches!(raw.kind, Kind::Paragraph) {
            walk.paragraph(node, raw.span, &mut tabs);
        } else if matches!(raw.kind, Kind::Heading(_)) {
            walk.inlines(node, raw.span);
        } else {
            walk.row(node, raw.span);
        }
        walk.pace.read += length(&raw);
    }
    let written = std::mem::replace(&mut doc.own, old);
    if doc.own.is_empty() {
        doc.own = written;
    } else {
        doc.own.push_str(&written);
    }
    let (mut sections, mut label) = (Vec::new(), String::new());
    for node in tabs {
        forms::tabs(doc, node, &mut sections, &mut label);
    }
}

impl<'s> Walk<'s, '_, '_> {
    fn cut(&self, span: Span) -> &'s str {
        let (start, end) = (at(span.start), at(span.end));
        let edge = self.src.len();
        let text = if start < edge {
            self.src.get(start..end)
        } else {
            self.old.get(start - edge..end.saturating_sub(edge))
        };
        text.unwrap_or_default()
    }

    /// The text at `span`, without the blanks at its ends, read into inlines under `parent`.
    fn inlines(&mut self, parent: u32, span: Span) {
        let text = self.cut(span);
        let (start, end) = trimmed(text.as_bytes(), 0, text.len());
        let text = text.get(start..end).unwrap_or_default();
        let base = at(span.start) + start;
        self.text(parent, text, base);
    }

    fn text(&mut self, parent: u32, text: &str, base: usize) {
        self.stacks.brackets.clear();
        Inline {
            doc: &mut *self.doc,
            refs: self.refs,
            stacks: &mut self.stacks,
            text,
            bytes: text.as_bytes(),
            base,
            edge: self.src.len() + self.old.len(),
            parent,
            pos: 0,
            from: 0,
            runs: super::Runs::Unread,
            apart: false,
            pace: self.pace,
            gfm: self.reads.gfm,
        }
        .run();
    }

    fn paragraph(&mut self, node: u32, mut span: Span, tabs: &mut Vec<u32>) {
        let raw = *self.doc.raw(node);
        let above = self.doc.raw(raw.parent).kind;
        if raw.prev != NONE {
            return self.inlines(node, span);
        }
        if self.reads.forms && matches!(above, Kind::Quote) {
            let text = self.cut(span);
            let taken = forms::marker(self.doc, raw.parent, node, text, at(span.start));
            span.start += pos(taken);
            if matches!(self.doc.raw(raw.parent).kind, Kind::Tabs) {
                tabs.push(raw.parent);
            }
            if self.doc.raw(node).parent == NONE {
                return;
            }
        } else if self.reads.gfm && matches!(above, Kind::Item(_)) {
            // GitHub's task: `[ ]` or `[x]`, then a space, where an item's text starts.
            let text = self.cut(span).as_bytes();
            let task = match text {
                [b'[', b' ', b']', after, ..] if is_blank(*after) => Task::Todo,
                [b'[', b'x' | b'X', b']', after, ..] if is_blank(*after) => Task::Done,
                _ => Task::None,
            };
            if task != Task::None {
                self.doc.raw_mut(raw.parent).kind = Kind::Item(task);
                span.start += 3;
            }
        }
        self.inlines(node, span);
    }

    /// A row's cells, as many as the table has columns: one short of them is filled with empty
    /// cells while there are any to spare, what is past them is left out.
    fn row(&mut self, node: u32, span: Span) {
        let table = *self.doc.raw(self.doc.raw(node).parent);
        let (first, columns) = (at(table.num), at(table.extra.start));
        let text = self.cut(span);
        // Each cell first, holding where its text is in the row's; then each one's inlines.
        let mut found = 0;
        cells(text.as_bytes(), |start, end| {
            if found < columns {
                let align = self.doc.aligns[first + found];
                let cut = Span::new(pos(start), pos(end));
                self.doc.push(node, Kind::Cell(align), cut);
                found += 1;
            }
        });
        let mut cell = self.doc.raw(node).first;
        while cell != NONE {
            let raw = std::mem::take(&mut self.doc.raw_mut(cell).span);
            let (start, end) = (at(raw.start), at(raw.end));
            let text = text.get(start..end).unwrap_or_default();
            if text.contains("\\|") {
                // A pipe written `\|` is the cell's text: read from a copy that has it so.
                let text = text.replace("\\|", "|");
                let base = self.src.len() + self.old.len() + self.doc.own.len();
                self.doc.own.push_str(&text);
                self.text(cell, &text, base);
            } else {
                self.text(cell, text, at(span.start) + start);
            }
            cell = self.doc.raw(cell).next;
        }
        for column in found..columns {
            if self.spare == 0 {
                break;
            }
            self.spare -= 1;
            self.doc.push(
                node,
                Kind::Cell(self.doc.aligns[first + column]),
                Span::default(),
            );
        }
    }
}
