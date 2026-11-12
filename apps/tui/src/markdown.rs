//! What the model writes, as styled rows: paragraphs wrapped at words, headings by their weight,
//! lists with their markers hanging (a bullet that says how deep the list is, a task's box where
//! its bullet would stand), quotes behind a rule with their source named under them, fences
//! (`markdown/fence.rs`), a formula as a terminal writes it (`markdown/math.rs`), footnotes by
//! number, a link's address after its text, and the forms of a reply (`markdown/forms.rs`). No
//! markup characters are left for the person to read past.

mod fence;
mod forms;
mod inline;
mod math;
mod table;
mod tree;

use markdown::{Kind, Node, Task};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::style;
use super::wrap::{fit, line_width, untabbed, width, wrap};
use crate::printable::printable;

/// `text` as rows of at most `width` columns.
pub(crate) fn render(text: &str, width: usize) -> Vec<Line<'static>> {
    let doc = markdown::parse(text);
    let mut out = Renderer {
        width: width.max(8),
        rows: vec![],
        first: None,
        rest: Line::default(),
        gap: None,
        depth: 0,
        step: false,
    };
    out.blocks(doc.root(), Under::Blocks);
    out.rows
}

struct Renderer {
    width: usize,
    rows: Vec<Line<'static>>,
    /// What the next row starts with, where it is not what the others do: a list's marker.
    first: Option<Line<'static>>,
    /// What every row starts with: a quote's rule, an item's indent.
    rest: Line<'static>,
    /// A blank row asked for and not yet drawn: it is, before the next row, if one comes.
    gap: Option<Line<'static>>,
    /// The plain lists this is inside.
    depth: usize,
    /// The next paragraph opens a step: its name in bold has a row of its own.
    step: bool,
}

/// The columns a nested block must leave to write in. A quote in a list in a quote would
/// otherwise leave none, and every row would run past the edge: past this depth the nesting is
/// no longer drawn, and the text still is.
const MIN_ROOM: usize = 4;

/// A list's bullet by how deep the list is: one shape, then the other, and round again.
const BULLETS: [&str; 2] = ["• ", "◦ "];

/// What a run of blocks is in: an item keeps its list close to its text.
#[derive(Clone, Copy)]
enum Under {
    Blocks,
    Item,
}

impl Renderer {
    /// A node's blocks, a blank row between them.
    fn blocks(&mut self, node: Node<'_>, under: Under) {
        let list = |kind| matches!(kind, Kind::List { .. });
        let quote = matches!(node.kind(), Kind::Quote);
        let (mut before, mut asked) = (None, false);
        for child in node.children() {
            let kind = child.kind();
            // A quote's last line may name its source: nothing parts the two.
            let source = (quote && child.next().is_none())
                .then(|| inline::source(child, before.is_some()))
                .flatten();
            let close = matches!(under, Under::Item) && (list(kind) || before.is_some_and(list));
            if before.is_some() && !close && source != Some(0) {
                self.blank();
                asked = true;
            }
            match source {
                Some(at) => self.cited(child, at),
                None => self.block(child),
            }
            before = Some(kind);
        }
        // A blank row asked for here and never drawn is not owed to what comes after.
        if asked {
            self.gap = None;
        }
    }

    fn block(&mut self, node: Node<'_>) {
        let italic = Style::new().add_modifier(Modifier::ITALIC);
        match node.kind() {
            // A formula alone in its paragraph (`$$ … $$` on one line) is set on rows of its own.
            Kind::Paragraph => match inline::alone(node, Kind::Math) {
                Some(formula) => self.formula(formula.text()),
                None => self.text(node, Style::new()),
            },
            // A heading by its depth: the first three stand out by weight, the rest by slant.
            Kind::Heading(1) => self.text(node, style::bold().add_modifier(Modifier::UNDERLINED)),
            Kind::Heading(2) => self.text(node, style::bold()),
            Kind::Heading(3) => self.text(node, style::bold().patch(italic)),
            Kind::Heading(_) => self.text(node, italic),
            Kind::Rule => self.rule(),
            Kind::Quote => {
                let rule = Span::styled("│ ", style::dim());
                self.within(rule.clone(), rule, |out| out.blocks(node, Under::Blocks));
            }
            Kind::List { ordered, .. } => {
                let start = u64::from(node.start());
                let bullet = BULLETS[self.depth % BULLETS.len()];
                self.depth += 1;
                self.items(node, |index| {
                    let marker = if ordered {
                        format!("{}. ", start + index as u64)
                    } else {
                        bullet.to_owned()
                    };
                    (marker, style::dim())
                });
                self.depth -= 1;
            }
            Kind::CodeBlock => self.fence(node),
            Kind::MathBlock => self.formula(node.text()),
            // An HTML block's text keeps its own line ends: each line is a row of its own.
            Kind::HtmlBlock => {
                for line in node.text().lines() {
                    self.put(&Line::from(Span::raw(printable(&untabbed(line)))));
                }
            }
            Kind::Table => self.table(node),
            Kind::Callout(kind) => self.callout(node, kind),
            Kind::Steps => self.steps(node),
            Kind::Tabs => self.tabs(node),
            Kind::Stats => self.stats(node),
            Kind::Pros => self.marked(node, "+ ", style::done()),
            Kind::Cons => self.marked(node, "- ", style::failed()),
            Kind::Details => self.details(node),
            Kind::Sources => self.sources(node),
            // A footnote is told at the end, behind the number it is named by.
            Kind::Footnotes => {
                self.items(node, |index| (format!("[{}] ", index + 1), style::dim()))
            }
            Kind::Document
            | Kind::Item(_)
            | Kind::Row { .. }
            | Kind::Cell(_)
            | Kind::Tab
            | Kind::Footnote => self.blocks(node, Under::Blocks),
            // Inlines stand in a paragraph, a heading or a cell, drawn with it.
            Kind::Text
            | Kind::SoftBreak
            | Kind::HardBreak
            | Kind::Emphasis
            | Kind::Strong
            | Kind::Strike
            | Kind::Code
            | Kind::Link
            | Kind::Image
            | Kind::Html
            | Kind::Math
            | Kind::FootnoteRef => {}
        }
    }

    /// A row, after the blank row asked for before it.
    fn emit(&mut self, row: Line<'static>) {
        if let Some(gap) = self.gap.take()
            && !self.rows.is_empty()
        {
            self.rows.push(gap);
        }
        self.rows.push(row);
    }

    fn rule(&mut self) {
        let rule = "─".repeat(self.room().min(40));
        self.put(&Line::from(Span::styled(rule, style::dim())));
    }

    /// The columns left of the row's prefix.
    fn room(&self) -> usize {
        self.width.saturating_sub(line_width(&self.rest)).max(1)
    }

    /// A row between two blocks, drawn before the next row if one comes: empty, or the rule of
    /// the quote they are in, with no space after it.
    fn blank(&mut self) {
        let mut row = self.rest.clone();
        while let Some(last) = row.spans.last_mut() {
            let kept = last.content.trim_end().len();
            if kept > 0 {
                let mut text = std::mem::take(&mut last.content).into_owned();
                text.truncate(kept);
                last.content = text.into();
                break;
            }
            row.spans.pop();
        }
        self.gap = Some(row);
    }

    /// What the next row starts with, and the rows it wraps onto.
    fn edges(&mut self) -> (Line<'static>, Line<'static>) {
        let first = self.first.take();
        (
            first.unwrap_or_else(|| self.rest.clone()),
            self.rest.clone(),
        )
    }

    /// `line` wrapped into rows behind their prefixes. A line with nothing to read draws nothing.
    fn put(&mut self, line: &Line<'static>) {
        if line.spans.iter().all(|span| span.content.trim().is_empty()) {
            return;
        }
        let (first, rest) = self.edges();
        for row in wrap(line, self.width, &first, &rest) {
            self.emit(row);
        }
    }

    /// `line` as it is, cut where the row ends: what is aligned by its columns, not its words.
    fn exact(&mut self, line: Line<'static>) {
        for row in fit(line, self.room()) {
            let (mut first, _) = self.edges();
            first.spans.extend(row.spans);
            self.emit(first);
        }
    }

    /// What `body` draws, behind `first` on its first row and `rest` on the others.
    fn within(&mut self, first: Span<'static>, rest: Span<'static>, body: impl FnOnce(&mut Self)) {
        // No room left to nest in: the text is drawn where it is, without the mark.
        if line_width(&self.rest) + width(&rest.content) + MIN_ROOM > self.width {
            return body(self);
        }
        let outer = self.rest.clone();
        let mut opening = self.first.take().unwrap_or_else(|| outer.clone());
        opening.spans.push(first);
        self.first = Some(opening);
        self.rest.spans.push(rest);
        body(self);
        // Nothing was drawn: the marker stands alone.
        if let Some(alone) = self.first.take() {
            self.emit(alone);
        }
        self.rest = outer;
    }

    /// A paragraph's or a heading's text, a row for each line a hard break ends. A step's name
    /// has a row of its own, with when it is done; a picture's caption the row under it.
    fn text(&mut self, node: Node<'_>, style: Style) {
        let mut lines = vec![vec![]];
        let step = std::mem::take(&mut self.step);
        if let Some(lead) = inline::lead(node).filter(|_| step) {
            inline::one(lead.title, style, &mut lines);
            if let Some(when) = lead.meta {
                inline::push(&mut lines, " ", style);
                inline::each(when.children(), style.patch(style::dim()), &mut lines);
            }
            lines.push(vec![]);
            inline::push(&mut lines, lead.text, style);
            inline::each(inline::from(lead.rest), style, &mut lines);
        } else if let Some(caption) = inline::caption(node) {
            inline::each(node.children().take(1), style, &mut lines);
            lines.push(vec![]);
            inline::each(caption.children(), style.patch(style::dim()), &mut lines);
        } else {
            inline::each(node.children(), style, &mut lines);
        }
        for line in lines {
            self.put(&Line::from(line));
        }
    }

    /// A quote's last paragraph, the line its inline `at` opens naming the quote's source.
    fn cited(&mut self, paragraph: Node<'_>, at: usize) {
        let mut lines = vec![vec![]];
        // The line ending before the source's line is not drawn.
        let quoted = paragraph.children().take(at.saturating_sub(1));
        inline::each(quoted, Style::new(), &mut lines);
        lines.push(vec![Span::styled("— ", style::dim())]);
        let mut rest = paragraph.children().skip(at);
        let name = rest.next().and_then(|first| inline::dash(first.text()));
        inline::push(&mut lines, name.unwrap_or_default(), style::dim());
        inline::each(rest, style::dim(), &mut lines);
        for line in lines {
            self.put(&Line::from(line));
        }
    }

    /// A list's items, each behind the marker `marker` gives it, a blank row between them where
    /// the list is written so.
    fn items(&mut self, list: Node<'_>, mut marker: impl FnMut(usize) -> (String, Style)) {
        let loose = matches!(list.kind(), Kind::List { tight: false, .. });
        let ordered = matches!(list.kind(), Kind::List { ordered: true, .. });
        for (index, item) in list.children().enumerate() {
            if index > 0 && loose {
                self.blank();
            }
            let (mut marker, mut style) = marker(index);
            // A task's box stands where the bullet would, after a number.
            let task = if let Kind::Item(task) = item.kind() {
                task
            } else {
                Task::None
            };
            let boxed = match task {
                Task::Done => Some(("✓ ", style::done())),
                Task::Todo => Some(("☐ ", style::dim())),
                Task::None => None,
            };
            if let Some((mark, colour)) = boxed {
                if !ordered {
                    marker.clear();
                }
                marker.push_str(mark);
                style = colour;
            }
            let pad = " ".repeat(width(&marker));
            self.within(Span::styled(marker, style), Span::raw(pad), |out| {
                out.blocks(item, Under::Item);
            });
        }
    }

    fn table(&mut self, node: Node<'_>) {
        let mut table = table::Table::new(node.aligns().to_vec());
        for row in node.children() {
            let cells = row.children().map(inline::line);
            table.rows.push(cells.collect());
        }
        let (first, rest) = self.edges();
        for row in table.render(self.width, &first, &rest) {
            self.emit(row);
        }
    }
}

#[cfg(test)]
#[path = "markdown_tests.rs"]
mod tests;
