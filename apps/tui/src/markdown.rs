//! What the model writes, as styled rows: paragraphs wrapped at words, bold headings, lists with
//! their markers hanging, quotes behind a rule, code in its own colour with its spaces kept, and a
//! link's address after its text. No markup characters are left for the person to read past.

mod table;

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::style;
use super::wrap::{hard, line_width, wrap};
use crate::printable::printable;

/// `text` as rows of at most `width` columns.
pub(crate) fn render(text: &str, width: usize) -> Vec<Line<'static>> {
    let mut out = Renderer {
        width: width.max(8),
        ..Renderer::default()
    };
    for event in Parser::new_ext(
        text,
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_TABLES,
    ) {
        out.event(event);
    }
    out.flush();
    out.rows
}

#[derive(Default)]
struct Renderer {
    width: usize,
    rows: Vec<Line<'static>>,
    line: Vec<Span<'static>>,
    styles: Vec<Style>,
    containers: Vec<Container>,
    /// The marker the next row of the innermost item starts with: `- `, `2. `.
    marker: Option<String>,
    /// The text of the code block being read.
    code: Option<String>,
    /// Each open link: its address, and where its text starts in `line`.
    links: Vec<(String, usize)>,
    /// A blank row goes before the next block.
    gap: bool,
    table: Option<table::Table>,
}

enum Container {
    Quote,
    List(Option<u64>),
    Item(usize),
}

impl Renderer {
    fn event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => match &mut self.code {
                Some(code) => code.push_str(&text),
                None => self.push(&text, self.style()),
            },
            Event::Code(text) => self.push(&text, self.style().patch(style::code())),
            Event::InlineHtml(text) => self.push(&text, self.style()),
            // An HTML block's text keeps its own line ends: each line is a row of its own.
            Event::Html(text) => {
                for line in text.lines() {
                    self.push(line, self.style());
                    self.flush();
                }
            }
            Event::SoftBreak => self.push(" ", self.style()),
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.block();
                let rule = "─".repeat(self.width.min(40));
                self.rows.push(Line::from(Span::styled(rule, style::dim())));
                self.gap = true;
            }
            Event::TaskListMarker(done) => {
                self.push(if done { "[x] " } else { "[ ] " }, style::dim())
            }
            Event::InlineMath(_) | Event::DisplayMath(_) | Event::FootnoteReference(_) => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "pulldown-cmark's tags, a foreign enum: the others draw as text"
        )]
        match tag {
            Tag::Table(alignments) => {
                self.block();
                self.table = Some(table::Table::new(alignments));
            }
            Tag::TableHead | Tag::TableRow => {
                if let Some(table) = &mut self.table {
                    table.rows.push(vec![]);
                }
            }
            Tag::Paragraph | Tag::HtmlBlock => self.block(),
            Tag::Heading { level, .. } => {
                self.block();
                let heading = if level == HeadingLevel::H1 {
                    style::bold().add_modifier(Modifier::UNDERLINED)
                } else {
                    style::bold()
                };
                self.styles.push(heading);
            }
            Tag::BlockQuote(_) => {
                self.block();
                self.containers.push(Container::Quote);
            }
            Tag::CodeBlock(_) => {
                self.block();
                // A fenced block's language is left out: the colour already says it is code.
                self.code = Some(String::new());
            }
            Tag::List(first) => {
                if self.in_item() {
                    self.flush();
                } else {
                    self.block();
                }
                self.containers.push(Container::List(first));
            }
            Tag::Item => {
                self.flush();
                let marker = match self.containers.last_mut() {
                    Some(Container::List(Some(n))) => {
                        let number = *n;
                        *n += 1;
                        format!("{number}. ")
                    }
                    _ => "- ".to_string(),
                };
                self.containers
                    .push(Container::Item(marker.chars().count()));
                self.marker = Some(marker);
            }
            Tag::Emphasis => self
                .styles
                .push(Style::new().add_modifier(Modifier::ITALIC)),
            Tag::Strong => self.styles.push(style::bold()),
            Tag::Strikethrough => self
                .styles
                .push(Style::new().add_modifier(Modifier::CROSSED_OUT)),
            Tag::Link { dest_url, .. } => {
                self.links.push((dest_url.to_string(), self.line.len()));
                self.styles
                    .push(Style::new().add_modifier(Modifier::UNDERLINED));
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "pulldown-cmark's tags, a foreign enum: the others close nothing"
        )]
        match tag {
            TagEnd::TableCell => {
                if let Some(table) = &mut self.table
                    && let Some(row) = table.rows.last_mut()
                {
                    row.push(Line::from(std::mem::take(&mut self.line)));
                }
            }
            TagEnd::Table => {
                if let Some(table) = self.table.take() {
                    let (first, rest) = self.prefixes();
                    self.rows.extend(table.render(self.width, &first, &rest));
                    self.marker = None;
                    self.gap = true;
                }
            }
            TagEnd::Paragraph | TagEnd::HtmlBlock => self.close_block(),
            TagEnd::Heading(_) => {
                self.styles.pop();
                self.close_block();
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.containers.pop();
                self.gap = true;
            }
            TagEnd::CodeBlock => {
                let code = self.code.take().unwrap_or_default();
                let (first, rest) = self.prefixes();
                let room = self.width.saturating_sub(line_width(&rest)).max(1);
                for line in code.trim_end_matches('\n').split('\n') {
                    for row in hard(&printable(line), room) {
                        // An item that opens with code shows its marker on the code's first row.
                        let mut row_line = if self.marker.take().is_some() {
                            first.clone()
                        } else {
                            rest.clone()
                        };
                        row_line.spans.push(Span::styled(row, style::code()));
                        self.rows.push(row_line);
                    }
                }
                self.gap = true;
            }
            TagEnd::List(_) => {
                self.flush();
                self.containers.pop();
                self.gap = !self.in_item();
            }
            TagEnd::Item => {
                self.flush();
                self.containers.pop();
                self.marker = None;
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.styles.pop();
            }
            TagEnd::Link => {
                self.styles.pop();
                if let Some((url, from)) = self.links.pop() {
                    let text: String = self.line[from..]
                        .iter()
                        .map(|s| s.content.as_ref())
                        .collect();
                    if !url.is_empty() && text != url && !url.starts_with('#') {
                        self.push(&format!(" ({url})"), style::dim());
                    }
                }
            }
            _ => {}
        }
    }

    /// A new block starts: what was open is finished, with a blank row between blocks.
    fn block(&mut self) {
        self.flush();
        if self.gap && !self.rows.is_empty() {
            let quoted = self
                .containers
                .iter()
                .any(|c| matches!(c, Container::Quote));
            self.rows.push(if quoted {
                self.prefixes().1
            } else {
                Line::default()
            });
        }
        self.gap = false;
    }

    fn close_block(&mut self) {
        self.flush();
        self.gap = true;
    }

    fn push(&mut self, text: &str, style: Style) {
        self.line.push(Span::styled(printable(text), style));
    }

    fn style(&self) -> Style {
        self.styles
            .iter()
            .fold(Style::new(), |all, style| all.patch(*style))
    }

    fn in_item(&self) -> bool {
        self.containers
            .iter()
            .any(|c| matches!(c, Container::Item(_)))
    }

    /// The prefixes of the next row and of the rows it wraps onto.
    fn prefixes(&self) -> (Line<'static>, Line<'static>) {
        let innermost = self
            .containers
            .iter()
            .rposition(|c| matches!(c, Container::Item(_)));
        let (mut first, mut rest) = (vec![], vec![]);
        for (at, container) in self.containers.iter().enumerate() {
            match container {
                Container::Quote => {
                    first.push(Span::styled("│ ", style::dim()));
                    rest.push(Span::styled("│ ", style::dim()));
                }
                Container::Item(indent) => {
                    let pad = " ".repeat(*indent);
                    match (&self.marker, Some(at) == innermost) {
                        (Some(marker), true) => {
                            first.push(Span::styled(marker.clone(), style::dim()))
                        }
                        _ => first.push(Span::raw(pad.clone())),
                    }
                    rest.push(Span::raw(pad));
                }
                Container::List(_) => {}
            }
        }
        (Line::from(first), Line::from(rest))
    }

    /// The row being built, wrapped into `rows`.
    fn flush(&mut self) {
        if self.line.is_empty() {
            return;
        }
        let (first, rest) = self.prefixes();
        let line = Line::from(std::mem::take(&mut self.line));
        self.rows.extend(wrap(&line, self.width, &first, &rest));
        self.marker = None;
        // A link open across a hard break goes on in the next row, which starts empty.
        for (_, from) in &mut self.links {
            *from = 0;
        }
    }
}

#[cfg(test)]
#[path = "markdown_tests.rs"]
mod tests;
