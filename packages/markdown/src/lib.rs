//! Markdown as a language model writes it, read into a tree: the blocks line by line
//! (`blocks.rs`), then each paragraph's text (`inline.rs`), every node in one arena (`tree.rs`,
//! read through `node.rs`), a reply's forms by their markers (`forms.rs`), and what a model is told to write them
//! (`prompt.rs`).
//!
//! The README follows, its example run as a test.
#![doc = include_str!("../README.md")]

mod blocks;
mod entity;
mod forms;
mod inline;
mod link;
mod node;
mod prompt;
mod refs;
mod scan;
mod tag;
mod tree;

pub use node::{Children, Node};
pub use prompt::prompt;
pub use tree::{Align, Callout, Document, Kind, Task};

/// What is read as Markdown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    /// The CommonMark specification, and nothing else.
    CommonMark,
    /// With GitHub's tables, strikethrough, task lists and bare links.
    Github,
    /// With the forms of an agent's reply too: callouts, steps, tabs.
    Reply,
}

/// The tree keeps a place in 32 bits: a text is read up to here.
const MOST: usize = 1 << 30;

/// `text` read as an agent's reply.
#[must_use]
pub fn parse(text: &str) -> Document<'_> {
    parse_as(text, Dialect::Reply)
}

/// `text` read as `dialect` has it. Any text is a document: nothing fails.
#[must_use]
pub fn parse_as(text: &str, dialect: Dialect) -> Document<'_> {
    let mut end = text.len().min(MOST);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let text = text.get(..end).unwrap_or_default();
    let reads = inline::Reads {
        gfm: dialect != Dialect::CommonMark,
        forms: dialect == Dialect::Reply,
    };
    let (mut doc, refs) = blocks::read(text, reads.gfm);
    inline::read(&mut doc, text, &refs, reads);
    doc
}

#[cfg(test)]
mod testing;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
