//! A paragraph's text as styled spans: a new line where a hard break ends one, a link's address
//! after its text.

use markdown::{Kind, Node};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::printable::printable;
use crate::style;
use crate::wrap::NO_BREAK;

type Lines = Vec<Vec<Span<'static>>>;

fn push(lines: &mut Lines, text: &str, style: Style) {
    if let Some(line) = lines.last_mut() {
        line.push(Span::styled(printable(text), style));
    }
}

/// `node`'s inlines, added to the last of `lines`.
pub(super) fn spans(node: Node<'_>, style: Style, lines: &mut Lines) {
    for child in node.children() {
        one(child, style, lines);
    }
}

/// One inline and what it holds.
pub(super) fn one(child: Node<'_>, style: Style, lines: &mut Lines) {
    match child.kind() {
        Kind::Text | Kind::Html => push(lines, child.text(), style),
        // Code keeps to one row where a row holds it: a command broken in two reads as two.
        Kind::Code => {
            let code = child.text().replace(' ', NO_BREAK.encode_utf8(&mut [0; 4]));
            push(lines, &code, style.patch(style::code()));
        }
        Kind::SoftBreak => push(lines, " ", style),
        Kind::HardBreak => lines.push(vec![]),
        Kind::Emphasis => spans(child, style.add_modifier(Modifier::ITALIC), lines),
        Kind::Strong => spans(child, style.patch(style::bold()), lines),
        Kind::Strike => spans(child, style.add_modifier(Modifier::CROSSED_OUT), lines),
        Kind::Link => link(child, style.add_modifier(Modifier::UNDERLINED), lines),
        Kind::Image => link(child, style, lines),
        // A block holds no block where inlines are read: whatever it is, its text.
        Kind::Document
        | Kind::Paragraph
        | Kind::Heading(_)
        | Kind::Rule
        | Kind::Quote
        | Kind::List { .. }
        | Kind::Item(_)
        | Kind::CodeBlock
        | Kind::HtmlBlock
        | Kind::Table
        | Kind::Row { .. }
        | Kind::Cell(_)
        | Kind::Callout(_)
        | Kind::Steps
        | Kind::Tabs
        | Kind::Tab
        | Kind::Stats
        | Kind::Pros
        | Kind::Cons
        | Kind::Details
        | Kind::Sources => spans(child, style, lines),
    }
}

/// A link's text, then its address where the text does not already say it.
fn link(node: Node<'_>, style: Style, lines: &mut Lines) {
    let (line, from) = (lines.len(), lines.last().map_or(0, Vec::len));
    spans(node, style, lines);
    // A link open across a hard break goes on in the next line, which starts empty.
    let from = if lines.len() == line { from } else { 0 };
    let shown = lines
        .last()
        .and_then(|line| line.get(from..))
        .unwrap_or_default();
    let text: String = shown.iter().map(|span| span.content.as_ref()).collect();
    let url = node.dest();
    if !url.is_empty() && text != url && !url.starts_with('#') {
        push(lines, &format!(" ({url})"), style::dim());
    }
}

/// A node's text as one line.
pub(super) fn line(node: Node<'_>) -> Line<'static> {
    let mut lines = vec![vec![]];
    spans(node, Style::new(), &mut lines);
    Line::from(lines.concat())
}
