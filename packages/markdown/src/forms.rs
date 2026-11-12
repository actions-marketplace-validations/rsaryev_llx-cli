//! The forms of an agent's reply: a quote whose first line is a marker (`> [!STEPS]`) is a
//! callout, stages in order, variants side by side, figures, the case for and against, a folded
//! section or a list of sources. A form is asked for by its marker and never guessed; one with
//! nothing it can take stays the quote it was written as.

use crate::scan::{is_blank, trimmed};
use crate::tree::{Callout, Document, Kind, NONE, Span, pos};

/// Every form by its marker's name: what is read here, and what the prompt teaches (`prompt.rs`).
pub(crate) const FORMS: [(&str, Kind); 12] = [
    ("NOTE", Kind::Callout(Callout::Note)),
    ("TIP", Kind::Callout(Callout::Tip)),
    ("IMPORTANT", Kind::Callout(Callout::Important)),
    ("WARNING", Kind::Callout(Callout::Warning)),
    ("CAUTION", Kind::Callout(Callout::Caution)),
    ("STEPS", Kind::Steps),
    ("TABS", Kind::Tabs),
    ("STATS", Kind::Stats),
    ("PROS", Kind::Pros),
    ("CONS", Kind::Cons),
    ("DETAILS", Kind::Details),
    ("SOURCES", Kind::Sources),
];

/// The longest marker's name, `IMPORTANT`.
const LONGEST: usize = 9;

pub(crate) fn form(name: &str) -> Option<Kind> {
    FORMS
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(name))
        .map(|(_, kind)| *kind)
}

/// What the form takes, right after `from`: a list, or the heading that opens the first tab.
fn takes(doc: &Document<'_>, kind: Kind, from: u32) -> bool {
    let next = doc.raw(from).next;
    if next == NONE {
        return false;
    }
    let found = doc.raw(next).kind;
    if matches!(kind, Kind::Tabs) {
        matches!(found, Kind::Heading(_))
    } else {
        matches!(found, Kind::List { .. })
    }
}

/// The marker a quote's first paragraph starts with, read: the quote made the form it names, its
/// title the rest of the marker's line. `text` is the paragraph's, addressed from `base`. Returns
/// how much of the text the marker took.
pub(crate) fn marker(
    doc: &mut Document<'_>,
    quote: u32,
    paragraph: u32,
    text: &str,
    base: usize,
) -> usize {
    let bytes = text.as_bytes();
    // Most quotes open with no marker, and a marker's name is short: neither is searched far.
    let name = bytes.strip_prefix(b"[!").unwrap_or_default();
    let mut within = name.iter().take(LONGEST + 1);
    let Some(close) = within.position(|byte| *byte == b']').map(|at| at + 2) else {
        return 0;
    };
    let Some(kind) = text.get(2..close).and_then(form) else {
        return 0;
    };
    let mut rest = bytes[close..].iter();
    let line = rest
        .position(|byte| *byte == b'\n')
        .map_or(text.len(), |at| close + at);
    let (start, end) = trimmed(bytes, close + 1, line);
    let callout = matches!(kind, Kind::Callout(_));
    // A callout's line with formatting on it is the first line of its text, not a title.
    let body = callout
        && bytes[start..end]
            .iter()
            .any(|byte| b"*_`[<~".contains(byte));
    let taken = if body {
        start
    } else {
        (line + 1).min(text.len())
    };
    let empty = bytes[taken..].iter().all(|byte| is_blank(*byte));
    if !callout && !matches!(kind, Kind::Details) {
        // The content is quoted with the marker, or follows the quote.
        let quoted = empty && takes(doc, kind, paragraph);
        let after = empty && doc.raw(paragraph).next == NONE && takes(doc, kind, quote);
        if !quoted && !after {
            return 0;
        }
        if after && !matches!(kind, Kind::Tabs) {
            let list = doc.raw(quote).next;
            doc.unlink(list);
            doc.append(quote, list);
        }
    }
    let raw = doc.raw_mut(quote);
    raw.kind = kind;
    if !body {
        raw.extra = Span::new(pos(base + start), pos(base + end));
    }
    if empty {
        doc.unlink(paragraph);
    }
    taken
}

/// The text under `node` as it reads with no formatting, onto `out`.
fn plain(doc: &Document<'_>, node: u32, out: &mut String) {
    let mut child = doc.raw(node).first;
    while child != NONE {
        let raw = doc.raw(child);
        if matches!(raw.kind, Kind::Text | Kind::Code) {
            out.push_str(doc.text(raw.span));
        } else if matches!(raw.kind, Kind::SoftBreak | Kind::HardBreak) {
            out.push(' ');
        } else {
            plain(doc, child, out);
        }
        child = raw.next;
    }
}

/// The sections of a `Tabs` node made its tabs: each heading of the first one's level opens a
/// tab and names it. Quoted with the marker, they are the node's children; else they follow it,
/// up to a shallower heading, a rule (which only ends them) or the end. `sections` and `label`
/// are the caller's to keep from one node to the next: what they hold is not read.
pub(crate) fn tabs(doc: &mut Document<'_>, node: u32, sections: &mut Vec<u32>, label: &mut String) {
    let quoted = doc.raw(node).first != NONE;
    let mut next = if quoted {
        doc.raw(node).first
    } else {
        doc.raw(node).next
    };
    let Kind::Heading(level) = doc.raw(next).kind else {
        return;
    };
    // The sections first, taken out of the tree: the tabs go where the quoted ones were.
    sections.clear();
    while next != NONE {
        let block = next;
        next = doc.raw(block).next;
        let kind = doc.raw(block).kind;
        if !quoted && matches!(kind, Kind::Heading(other) if other < level) {
            break;
        }
        doc.unlink(block);
        if !quoted && matches!(kind, Kind::Rule) {
            break;
        }
        sections.push(block);
    }
    let mut tab = NONE;
    for block in sections.iter().copied() {
        if doc.raw(block).kind == Kind::Heading(level) {
            label.clear();
            plain(doc, block, label);
            let start = doc.src.len() + doc.own.len();
            doc.own.push_str(label);
            tab = doc.push(node, Kind::Tab, Span::default());
            doc.raw_mut(tab).extra = Span::new(pos(start), pos(start + label.len()));
        } else {
            doc.append(tab, block);
        }
    }
}

#[cfg(test)]
#[path = "forms_tests.rs"]
mod tests;
