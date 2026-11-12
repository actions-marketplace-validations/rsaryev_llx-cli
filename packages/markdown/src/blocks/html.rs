//! The seven kinds of HTML block: what opens each, and what closes the first five.

use super::{Blocks, Data};
use crate::scan::{find, is_space};
use crate::tag;
use crate::tree::at;

/// The elements a line may open a block of HTML with, whatever follows them.
const BLOCKS: &[&str] = &[
    "address",
    "article",
    "aside",
    "base",
    "basefont",
    "blockquote",
    "body",
    "caption",
    "center",
    "col",
    "colgroup",
    "dd",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "frame",
    "frameset",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hr",
    "html",
    "iframe",
    "legend",
    "li",
    "link",
    "main",
    "menu",
    "menuitem",
    "nav",
    "noframes",
    "ol",
    "optgroup",
    "option",
    "p",
    "param",
    "search",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "title",
    "tr",
    "track",
    "ul",
];

/// The elements whose text is kept as written up to their closing tag.
const VERBATIM: &[&str] = &["script", "pre", "textarea", "style"];

fn named(name: &[u8], names: &[&str]) -> bool {
    names
        .iter()
        .any(|known| known.as_bytes().eq_ignore_ascii_case(name))
}

/// The kind of HTML block `line` (which starts with `<`) opens.
pub(super) fn opens(line: &[u8]) -> Option<u8> {
    let rest = line.get(1..)?;
    if rest.starts_with(b"!--") {
        return Some(2);
    }
    if rest.starts_with(b"?") {
        return Some(3);
    }
    if rest.starts_with(b"![CDATA[") {
        return Some(5);
    }
    if rest.starts_with(b"!") {
        return rest.get(1)?.is_ascii_alphabetic().then_some(4);
    }
    let closing = rest.starts_with(b"/");
    let start = usize::from(closing);
    let len = rest[start..]
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric())
        .count();
    let name = &rest[start..start + len];
    let after = rest.get(start + len).copied();
    let ends = after.is_none_or(|byte| is_space(byte) || byte == b'>');
    if !closing && ends && named(name, VERBATIM) {
        return Some(1);
    }
    if named(name, BLOCKS) && (ends || rest[start + len..].starts_with(b"/>")) {
        return Some(6);
    }
    let end = tag::tag(line, 0)?;
    let alone = line[end..].iter().all(|byte| is_space(*byte));
    (alone && !line[..end].contains(&b'\n')).then_some(7)
}

/// Whether `line` holds `pattern`, which ends in a `>`: each `>` from `from` on looked at for
/// what is before it.
fn holds(line: &[u8], pattern: &[u8], from: usize) -> bool {
    let lead = pattern.len() - 1;
    let mut from = from.max(lead);
    while let Some(end) = find(line, from, b'>') {
        if line[end - lead..end] == pattern[..lead] {
            return true;
        }
        from = end + 1;
    }
    false
}

/// Whether `line` holds the closing tag of an element whose text is kept as written.
fn closes_verbatim(line: &[u8]) -> bool {
    let mut from = 0;
    while let Some(start) = find(line, from, b'<') {
        let rest = &line[start + 1..];
        let named = |name: &&str| {
            rest.get(1..=name.len())
                .is_some_and(|tag| tag.eq_ignore_ascii_case(name.as_bytes()))
                && rest.get(name.len() + 1) == Some(&b'>')
        };
        if rest.first() == Some(&b'/') && VERBATIM.iter().any(named) {
            return true;
        }
        from = start + 1;
    }
    false
}

/// What reading a line learned of its first `>`: a line that may close an HTML block is looked
/// through for one as its end is found, in the same pass.
#[derive(Clone, Copy)]
pub(super) enum Angle {
    Unknown,
    None,
    At(usize),
}

impl Blocks<'_> {
    /// Whether the line that starts at `start` may close an HTML block: one of the first five
    /// kinds is open, or the line may open one.
    pub(super) fn angled(&self, start: usize) -> bool {
        let mut line = self.bytes[start..].iter().take(4);
        matches!(self.info[at(self.tip)].data, Data::Html(1..=5))
            || line.find(|byte| **byte != b' ') == Some(&b'<')
    }

    /// Whether the line holds what closes an HTML block of `kind`, one of the first five.
    pub(super) fn closes(&self, kind: u8) -> bool {
        let start = self.offset.min(self.end);
        let line = &self.bytes[start..self.end];
        let from = match self.angle {
            Angle::Unknown => 0,
            // Each of the five ends in a `>`.
            Angle::None => return false,
            Angle::At(first) => first.saturating_sub(start),
        };
        match kind {
            1 => closes_verbatim(line),
            2 => holds(line, b"-->", from),
            3 => holds(line, b"?>", from),
            4 => holds(line, b">", from),
            5 => holds(line, b"]]>", from),
            _ => false,
        }
    }
}

#[cfg(test)]
#[path = "html_tests.rs"]
mod tests;
