//! A paragraph's text as styled spans: a new line where a hard break ends one, a link's address
//! after its text. And what a paragraph is read as by how it is written: a step's or a figure's
//! bold lead, the line that names a quote's source, a picture's caption.

use markdown::{Kind, Node};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::math;
use crate::printable::printable;
use crate::style;
use crate::wrap::NO_BREAK;

type Lines = Vec<Vec<Span<'static>>>;

pub(super) fn push(lines: &mut Lines, text: &str, style: Style) {
    if let Some(line) = lines.last_mut() {
        line.push(Span::styled(printable(text), style));
    }
}

/// Each of `nodes`, added to the last of `lines`.
pub(super) fn each<'d>(nodes: impl Iterator<Item = Node<'d>>, style: Style, lines: &mut Lines) {
    for node in nodes {
        one(node, style, lines);
    }
}

/// `node` and the inlines after it.
pub(super) fn from(node: Option<Node<'_>>) -> impl Iterator<Item = Node<'_>> {
    std::iter::successors(node, |node| node.next())
}

fn spans(node: Node<'_>, style: Style, lines: &mut Lines) {
    each(node.children(), style, lines);
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
        // A formula keeps to one row where a row holds it, as code does.
        Kind::Math => {
            let formula = math::text(child.text()).replace(' ', NO_BREAK.encode_utf8(&mut [0; 4]));
            push(lines, &formula, style.add_modifier(Modifier::ITALIC));
        }
        Kind::FootnoteRef => push(lines, &format!("[{}]", child.start()), style::dim()),
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
        | Kind::Sources
        | Kind::MathBlock
        | Kind::Footnotes
        | Kind::Footnote => spans(child, style, lines),
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

/// The one inline of `paragraph`, when it holds nothing else and is of `kind`.
pub(super) fn alone(paragraph: Node<'_>, kind: Kind) -> Option<Node<'_>> {
    let mut inlines = paragraph.children();
    let only = inlines.next().filter(|only| only.kind() == kind)?;
    inlines.next().is_none().then_some(only)
}

/// A node's text as one line.
pub(super) fn line(node: Node<'_>) -> Line<'static> {
    let mut lines = vec![vec![]];
    spans(node, Style::new(), &mut lines);
    Line::from(lines.concat())
}

/// `lines` as one, without the spaces at its two ends: a figure, set in a column.
pub(super) fn trimmed(lines: &[Vec<Span<'static>>]) -> Line<'static> {
    let mut line = lines.concat();
    while line
        .last()
        .is_some_and(|span| span.content.trim().is_empty())
    {
        line.pop();
    }
    let blank = |span: &Span<'_>| span.content.trim().is_empty();
    let lead = line.iter().take_while(|span| blank(span)).count();
    line.drain(..lead);
    if let Some(first) = line.first_mut() {
        first.content = first.content.trim_start().to_owned().into();
    }
    if let Some(last) = line.last_mut() {
        last.content = last.content.trim_end().to_owned().into();
    }
    Line::from(line)
}

/// A paragraph that opens in bold, as a step and a figure are written: `**Tag it** *a minute*
/// and what is said of it`.
pub(super) struct Lead<'d> {
    /// The bold it opens with: a step's name, a figure's number.
    pub(super) title: Node<'d>,
    /// The emphasis right after it: when a step is done.
    pub(super) meta: Option<Node<'d>>,
    /// What follows, past what parts it from the lead: the rest of a text, then the inlines
    /// after it.
    pub(super) text: &'d str,
    pub(super) rest: Option<Node<'d>>,
}

/// `at` and what follows it, past the text that only parts a lead from the rest: ` — `, `: `,
/// a line's end. What is left of the first text, and the inline after it.
pub(super) fn parted(mut at: Option<Node<'_>>) -> (&str, Option<Node<'_>>) {
    let blank = [' ', '\t'];
    while let Some(node) = at {
        let kind = node.kind();
        if matches!(kind, Kind::Text) {
            let text = node.text().trim_start_matches(blank);
            let text = text.strip_prefix(['—', '–', '·', ':', '|', '-']);
            let text = text.unwrap_or_else(|| node.text());
            let text = text.trim_start_matches(blank);
            if !text.is_empty() {
                return (text, node.next());
            }
        } else if !matches!(kind, Kind::SoftBreak | Kind::HardBreak) {
            return ("", at);
        }
        at = node.next();
    }
    ("", None)
}

/// A paragraph's bold lead and what follows it, when it opens with one.
pub(super) fn lead(paragraph: Node<'_>) -> Option<Lead<'_>> {
    let (before, title) = parted(paragraph.first());
    let title = title.filter(|node| before.is_empty() && matches!(node.kind(), Kind::Strong))?;
    let (mut text, mut rest) = parted(title.next());
    let meta = rest.filter(|node| text.is_empty() && matches!(node.kind(), Kind::Emphasis));
    if let Some(meta) = meta {
        (text, rest) = parted(meta.next());
    }
    Some(Lead {
        title,
        meta,
        text,
        rest,
    })
}

/// The name a line gives a quote's source, when it opens as one does: `— Tim Peters`.
pub(super) fn dash(text: &str) -> Option<&str> {
    let named = text.strip_prefix('—').or_else(|| text.strip_prefix("--"))?;
    let name = named.trim_start_matches([' ', '\t']);
    (name.len() < named.len()).then_some(name)
}

/// Where in a quote's last block the line that names its source opens: the inline's place among
/// the paragraph's. `quoted` says something stands before the block: a quote that is nothing but
/// the line has no quotation to name the source of.
pub(super) fn source(block: Node<'_>, quoted: bool) -> Option<usize> {
    if !matches!(block.kind(), Kind::Paragraph) {
        return None;
    }
    let mut opens = true;
    let mut found = None;
    for (at, node) in block.children().enumerate() {
        if opens && matches!(node.kind(), Kind::Text) && dash(node.text()).is_some() {
            found = Some(at);
        }
        opens = matches!(node.kind(), Kind::SoftBreak | Kind::HardBreak);
    }
    found.filter(|at| *at > 0 || quoted)
}

/// The caption a picture carries: the emphasis that is all of the line right under it.
pub(super) fn caption(paragraph: Node<'_>) -> Option<Node<'_>> {
    let image = paragraph.first()?;
    let ended = image.next()?;
    let caption = ended.next()?;
    let is = matches!(
        (image.kind(), ended.kind(), caption.kind()),
        (Kind::Image, Kind::SoftBreak, Kind::Emphasis)
    );
    (is && caption.next().is_none()).then_some(caption)
}
