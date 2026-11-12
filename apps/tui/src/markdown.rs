//! What the model writes, as styled rows: paragraphs wrapped at words, bold headings, lists with
//! their markers hanging (a task's box where its bullet would stand), quotes behind a rule, code
//! in its own colour with its spaces kept and its tabs as columns, a link's address after its text, and the forms of a reply (`markdown/forms.rs`). No markup
//! characters are left for the person to read past.

mod forms;
mod inline;
mod table;

use markdown::{Kind, Node, Task};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::style;
use super::wrap::{hard, line_width, untabbed, width, wrap};
use crate::printable::printable;

/// `text` as rows of at most `width` columns.
pub(crate) fn render(text: &str, width: usize) -> Vec<Line<'static>> {
    let doc = markdown::parse(text);
    let mut out = Renderer {
        width: width.max(8),
        rows: vec![],
        first: None,
        rest: Line::default(),
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
    /// The next paragraph opens a step: a title in bold alone on its line stays alone on its row.
    step: bool,
}

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
        let mut before = None;
        for child in node.children() {
            let kind = child.kind();
            let close = matches!(under, Under::Item) && (list(kind) || before.is_some_and(list));
            if before.is_some() && !close {
                self.blank();
            }
            self.block(child);
            before = Some(kind);
        }
    }

    fn block(&mut self, node: Node<'_>) {
        match node.kind() {
            Kind::Paragraph => self.text(node, Style::new()),
            Kind::Heading(1) => self.text(node, style::bold().add_modifier(Modifier::UNDERLINED)),
            Kind::Heading(_) => self.text(node, style::bold()),
            Kind::Rule => {
                let rule = "─".repeat(self.room().min(40));
                self.put(&Line::from(Span::styled(rule, style::dim())));
            }
            Kind::Quote => {
                let rule = Span::styled("│ ", style::dim());
                self.within(rule.clone(), rule, |out| out.blocks(node, Under::Blocks));
            }
            Kind::List { ordered, .. } => {
                let start = node.start();
                self.items(node, |index| {
                    let marker = if ordered {
                        format!("{}. ", u64::from(start) + index as u64)
                    } else {
                        "- ".to_owned()
                    };
                    (marker, style::dim())
                });
            }
            Kind::CodeBlock => self.code(node.text()),
            // An HTML block's text keeps its own line ends: each line is a row of its own.
            Kind::HtmlBlock => {
                for line in node.text().lines() {
                    self.put(&Line::from(Span::raw(printable(line))));
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
            Kind::Document | Kind::Item(_) | Kind::Row { .. } | Kind::Cell(_) | Kind::Tab => {
                self.blocks(node, Under::Blocks);
            }
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
            | Kind::Html => {}
        }
    }

    /// The columns left of the row's prefix.
    fn room(&self) -> usize {
        self.width.saturating_sub(line_width(&self.rest)).max(1)
    }

    /// A row between two blocks: empty, or the rule of the quote they are in, with no space
    /// after it.
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
        self.rows.push(row);
    }

    /// What the next row starts with, and the rows it wraps onto.
    fn edges(&mut self) -> (Line<'static>, Line<'static>) {
        let first = self.first.take();
        (
            first.unwrap_or_else(|| self.rest.clone()),
            self.rest.clone(),
        )
    }

    /// `line` wrapped into rows behind their prefixes.
    fn put(&mut self, line: &Line<'static>) {
        if line.spans.is_empty() {
            return;
        }
        let (first, rest) = self.edges();
        self.rows.extend(wrap(line, self.width, &first, &rest));
    }

    /// What `body` draws, behind `first` on its first row and `rest` on the others.
    fn within(&mut self, first: Span<'static>, rest: Span<'static>, body: impl FnOnce(&mut Self)) {
        let outer = self.rest.clone();
        let mut opening = self.first.take().unwrap_or_else(|| outer.clone());
        opening.spans.push(first);
        self.first = Some(opening);
        self.rest.spans.push(rest);
        body(self);
        // Nothing was drawn: the marker stands alone.
        self.rows.extend(self.first.take());
        self.rest = outer;
    }

    /// A paragraph's or a heading's text, a row for each line a hard break ends.
    fn text(&mut self, node: Node<'_>, style: Style) {
        let mut lines = vec![vec![]];
        let mut next = node.first();
        let title = next.filter(|first| matches!(first.kind(), Kind::Strong));
        let broken = title
            .and_then(Node::next)
            .filter(|after| matches!(after.kind(), Kind::SoftBreak));
        if let (true, Some(title), Some(end)) = (std::mem::take(&mut self.step), title, broken) {
            inline::one(title, style, &mut lines);
            lines.push(vec![]);
            next = end.next();
        }
        while let Some(inline) = next {
            inline::one(inline, style, &mut lines);
            next = inline.next();
        }
        for line in lines {
            self.put(&Line::from(line));
        }
    }

    /// Code in its colour, its spaces kept and its tabs as columns, broken anywhere when a line is
    /// too wide.
    fn code(&mut self, code: &str) {
        let room = self.room();
        for line in code.trim_end_matches('\n').split('\n') {
            for row in hard(&printable(&untabbed(line)), room) {
                // An item that opens with code shows its marker on the code's first row.
                let (mut first, _) = self.edges();
                first.spans.push(Span::styled(row, style::code()));
                self.rows.push(first);
            }
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
                Task::Done => Some(("[x] ", style::done())),
                Task::Todo => Some(("[ ] ", style::dim())),
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
        self.rows.extend(table.render(self.width, &first, &rest));
    }
}

#[cfg(test)]
#[path = "markdown_tests.rs"]
mod tests;
