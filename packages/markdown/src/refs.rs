//! Link reference definitions (`[label]: destination "title"`), kept by their label for the
//! links that name them.

use std::collections::HashMap;

use crate::entity::unescape;
use crate::link::{self, spnl};
use crate::scan::is_space;
use crate::tree::{Span, pos};

#[derive(Clone, Copy)]
pub(crate) struct Def {
    pub(crate) dest: Span,
    pub(crate) title: Option<Span>,
}

#[derive(Default)]
pub(crate) struct Refs {
    defs: HashMap<String, Def>,
}

/// A label as two labels are compared: its whitespace collapsed, its case folded.
pub(crate) fn normalize(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    for word in label.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&word.to_lowercase().to_uppercase());
    }
    out
}

/// A label of at most this many bytes, all ASCII, is compared with no allocation.
const SHORT: usize = 64;

/// What `normalize` makes of a short label of ASCII alone, written into `out`: most labels.
fn folded<'o>(label: &str, out: &'o mut [u8; SHORT]) -> Option<&'o str> {
    if label.len() > SHORT || !label.is_ascii() {
        return None;
    }
    let (mut len, mut gap) = (0, false);
    for byte in label.bytes() {
        if byte.is_ascii_whitespace() || byte == 0x0b {
            gap = len > 0;
            continue;
        }
        if gap {
            out[len] = b' ';
            len += 1;
            gap = false;
        }
        out[len] = byte.to_ascii_uppercase();
        len += 1;
    }
    str::from_utf8(&out[..len]).ok()
}

/// What `find` makes of `label` as a key of a table by labels: folded in place when it is short.
pub(crate) fn keyed<R>(label: &str, find: impl FnOnce(&str) -> R) -> R {
    let mut short = [0; SHORT];
    match folded(label, &mut short) {
        Some(key) => find(key),
        None => find(&normalize(label)),
    }
}

/// Past the spaces that end a line at `at`, when nothing else is left on it.
fn line_end(text: &[u8], mut at: usize) -> Option<usize> {
    while text.get(at).is_some_and(|byte| is_space(*byte)) {
        at += 1;
    }
    match text.get(at) {
        None => Some(at),
        Some(b'\n') => Some(at + 1),
        Some(_) => None,
    }
}

impl Refs {
    pub(crate) fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub(crate) fn get(&self, label: &str) -> Option<Def> {
        if self.defs.is_empty() {
            return None;
        }
        keyed(label, |key| self.defs.get(key).copied())
    }

    /// The definition `text` starts with, kept (the first of a label wins), its destination and
    /// title written to `own`, which is addressed from `edge`: how much of the text it took.
    pub(crate) fn define(&mut self, text: &str, own: &mut String, edge: usize) -> Option<usize> {
        let bytes = text.as_bytes();
        let label = link::label(bytes, 0)?;
        if bytes.get(label) != Some(&b':') {
            return None;
        }
        let dest = link::destination(bytes, spnl(bytes, label + 1))?;
        let spaced = spnl(bytes, dest.after);
        let mut title = (spaced > dest.after)
            .then(|| link::title(bytes, spaced))
            .flatten();
        let mut end = title.and_then(|title| line_end(bytes, title.after));
        if end.is_none() {
            title = None;
            end = line_end(bytes, dest.after);
        }
        let end = end?;
        let key = normalize(text.get(1..label - 1)?);
        if key.is_empty() {
            return None;
        }
        self.defs.entry(key).or_insert_with(|| {
            let mut write = |start: usize, end: usize| {
                let from = edge + own.len();
                unescape(text.get(start..end).unwrap_or_default(), own);
                Span::new(pos(from), pos(edge + own.len()))
            };
            Def {
                dest: write(dest.start, dest.end),
                title: title.map(|title| write(title.start, title.end)),
            }
        });
        Some(end)
    }
}

#[cfg(test)]
#[path = "refs_tests.rs"]
mod tests;
