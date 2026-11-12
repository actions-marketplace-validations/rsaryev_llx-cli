//! Each parser behind one name, asked for the same thing: the whole text read once, with
//! GitHub's tables, strikethrough, task lists and alerts on.

use std::hint::black_box;

use comrak::{Arena, Options as ComrakOptions};
use pulldown_cmark::{Options, Parser};

/// A way to read a text, and what it leaves the caller with.
pub struct Reader {
    pub name: &'static str,
    pub gives: &'static str,
    pub read: fn(&str) -> usize,
}

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM
}

/// Ours: the tree, built.
fn ours(text: &str) -> usize {
    let doc = markdown::parse(text);
    black_box(&doc);
    doc.blocks().count()
}

fn visit(node: markdown::Node<'_>) -> usize {
    let own = node.text().len() + node.dest().len() + node.title().len() + node.info().len();
    node.children().map(visit).sum::<usize>() + own + 1
}

/// Ours: the tree built, then every node of it visited and its texts read, as a renderer does.
fn ours_walked(text: &str) -> usize {
    visit(markdown::parse(text).root())
}

/// pulldown-cmark: every event pulled and dropped; nothing is kept to draw from.
fn pulldown(text: &str) -> usize {
    Parser::new_ext(text, options()).count()
}

/// pulldown-cmark: the events kept, the nearest it has to a tree one can walk twice.
fn pulldown_kept(text: &str) -> usize {
    let events: Vec<_> = Parser::new_ext(text, options()).collect();
    black_box(&events);
    events.len()
}

/// comrak: its tree, built in an arena.
fn comrak(text: &str) -> usize {
    let arena = Arena::new();
    let mut options = ComrakOptions::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.autolink = true;
    options.extension.alerts = true;
    let root = comrak::parse_document(&arena, text, &options);
    black_box(root);
    root.children().count()
}

pub const OURS: Reader = Reader {
    name: "markdown",
    gives: "a tree",
    read: ours,
};
pub const OURS_WALKED: Reader = Reader {
    name: "markdown, walked",
    gives: "a tree, every node visited",
    read: ours_walked,
};
pub const PULLDOWN: Reader = Reader {
    name: "pulldown-cmark",
    gives: "events, dropped",
    read: pulldown,
};
pub const PULLDOWN_KEPT: Reader = Reader {
    name: "pulldown-cmark, kept",
    gives: "events in a `Vec`",
    read: pulldown_kept,
};
pub const COMRAK: Reader = Reader {
    name: "comrak",
    gives: "a tree",
    read: comrak,
};

pub const ALL: [Reader; 5] = [OURS, OURS_WALKED, PULLDOWN, PULLDOWN_KEPT, COMRAK];
