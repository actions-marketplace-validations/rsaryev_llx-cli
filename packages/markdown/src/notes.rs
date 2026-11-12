//! Footnotes (`[^label]: text`, referred to as `[^label]`): each definition kept by its label as
//! the block pass reads it, numbered as the inline pass first meets a reference to it, then the
//! ones referred to gathered at the document's end in that order. A definition leaves the flow
//! where it was written; one never referred to leaves the tree.

use std::collections::HashMap;

use crate::refs::{keyed, normalize};
use crate::scan::is_blank;
use crate::tree::{Document, Kind, Span, pos};

#[derive(Clone, Copy)]
struct Note {
    node: u32,
    /// Its place among the footnotes referred to, from 1; 0 before it is referred to.
    number: u32,
}

#[derive(Default)]
pub(crate) struct Notes {
    /// The first definition of each label.
    by_label: HashMap<String, Note>,
    /// Every definition, in the order written.
    all: Vec<u32>,
    /// The definitions referred to, in the order of their first reference.
    order: Vec<u32>,
}

/// Where a label that starts at `start` can end: the first `]` or blank from there, or the
/// text's end.
pub(crate) fn stop(text: &[u8], start: usize) -> usize {
    let rest = text.get(start..).unwrap_or_default();
    let len = rest
        .iter()
        .position(|byte| *byte == b']' || is_blank(*byte));
    start + len.unwrap_or(rest.len())
}

/// The label of a footnote whose `[^` is right before `start`, ended at `stop`: at a `]`, with a
/// character and no blank.
pub(crate) fn label(text: &[u8], start: usize, stop: usize) -> Option<&str> {
    let label = text.get(start..stop).filter(|label| !label.is_empty())?;
    (text.get(stop) == Some(&b']'))
        .then(|| str::from_utf8(label).ok())
        .flatten()
}

impl Notes {
    pub(crate) fn is_empty(&self) -> bool {
        self.all.is_empty()
    }

    /// A definition of `label` at `node`: the first of a label is the one told.
    pub(crate) fn define(&mut self, label: &str, node: u32) {
        self.all.push(node);
        let note = Note { node, number: 0 };
        self.by_label.entry(normalize(label)).or_insert(note);
    }

    /// The number of the footnote `label` names, given it now when this is its first reference;
    /// `None` when no footnote has that label.
    pub(crate) fn refer(&mut self, label: &str) -> Option<u32> {
        let order = &mut self.order;
        let by_label = &mut self.by_label;
        keyed(label, |key| {
            let note = by_label.get_mut(key)?;
            if note.number == 0 {
                order.push(note.node);
                note.number = pos(order.len());
            }
            Some(note.number)
        })
    }

    /// The footnote referred to `nth`, from 0, once one is.
    pub(crate) fn told(&self, nth: usize) -> Option<u32> {
        self.order.get(nth).copied()
    }

    /// Every definition taken out of the flow; those referred to put at the document's end, each
    /// with its number.
    pub(crate) fn gather(&self, doc: &mut Document<'_>) {
        for node in &self.all {
            doc.unlink(*node);
        }
        if self.order.is_empty() {
            return;
        }
        let list = doc.push(0, Kind::Footnotes, Span::default());
        for (place, node) in self.order.iter().enumerate() {
            doc.append(list, *node);
            doc.raw_mut(*node).num = pos(place + 1);
        }
    }
}

#[cfg(test)]
#[path = "notes_tests.rs"]
mod tests;
