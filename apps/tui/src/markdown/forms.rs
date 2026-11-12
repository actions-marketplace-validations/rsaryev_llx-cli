//! The forms of a reply, drawn for a terminal: a callout behind a bar of its colour, steps
//! numbered down a rail, tabs one under another each under its label, figures on a row (a list
//! where no row holds them), the case for and against by their markers, a section under its
//! title, sources by number.

use markdown::{Callout, Kind, Node};
use ratatui::text::{Line, Span};

use super::{Renderer, Under};
use crate::printable::printable;
use crate::style;
use crate::wrap::{line_width, width};

/// The paragraph that is all an item holds.
fn only_text(item: Node<'_>) -> Option<Node<'_>> {
    let text = item.first().filter(|only| only.next().is_none());
    text.filter(|text| matches!(text.kind(), Kind::Paragraph))
}

impl Renderer {
    /// A form's title, in bold.
    fn title(&mut self, node: Node<'_>) {
        if !node.title().is_empty() {
            self.put(&Line::from(Span::styled(
                printable(node.title()),
                style::bold(),
            )));
        }
    }

    pub(super) fn callout(&mut self, node: Node<'_>, kind: Callout) {
        let (name, colour) = match kind {
            Callout::Note => ("Note", style::code()),
            Callout::Tip => ("Tip", style::done()),
            Callout::Important => ("Important", style::bold()),
            Callout::Warning => ("Warning", style::waiting()),
            Callout::Caution => ("Caution", style::failed()),
        };
        let bar = Span::styled("▌ ", colour);
        self.within(bar.clone(), bar, |out| {
            let mut head = vec![Span::styled(name, colour.patch(style::bold()))];
            if !node.title().is_empty() {
                head.push(Span::styled(" · ", style::dim()));
                head.push(Span::styled(printable(node.title()), style::bold()));
            }
            out.put(&Line::from(head));
            out.blocks(node, Under::Blocks);
        });
    }

    /// Each step its number, a rail down to the next one.
    pub(super) fn steps(&mut self, node: Node<'_>) {
        self.title(node);
        let Some(list) = node.first() else { return };
        let (start, count) = (u64::from(list.start().max(1)), list.children().count());
        let wide = (start + count as u64).to_string().len();
        for (index, item) in list.children().enumerate() {
            let rail = Span::styled(format!("{:<wide$} ", "│"), style::dim());
            if index > 0 {
                let mut joint = self.rest.clone();
                joint.spans.push(Span::styled("│", style::dim()));
                self.rows.push(joint);
            }
            let number = format!("{:<wide$} ", start + index as u64);
            let rest = if index + 1 == count {
                Span::raw(" ".repeat(wide + 1))
            } else {
                rail
            };
            self.step = true;
            self.within(
                Span::styled(number, style::done().patch(style::bold())),
                rest,
                |out| {
                    out.blocks(item, Under::Item);
                },
            );
            self.step = false;
        }
    }

    /// A terminal shows every variant: each under a rule that holds its label.
    pub(super) fn tabs(&mut self, node: Node<'_>) {
        self.title(node);
        for (index, tab) in node.children().enumerate() {
            if index > 0 {
                self.blank();
            }
            let label = printable(tab.title());
            let rule = "─".repeat(self.room().min(40).saturating_sub(width(&label) + 4));
            self.put(&Line::from(vec![
                Span::styled("─ ", style::dim()),
                Span::styled(label, style::bold()),
                Span::styled(format!(" {rule}"), style::dim()),
            ]));
            self.blocks(tab, Under::Blocks);
        }
    }

    /// Figures side by side while the row holds them, else a list of them.
    pub(super) fn stats(&mut self, node: Node<'_>) {
        let Some(list) = node.first() else { return };
        if !list.children().all(|item| only_text(item).is_some()) {
            return self.marked(node, "- ", style::dim());
        }
        let figures: Vec<Line<'static>> = list
            .children()
            .filter_map(only_text)
            .map(super::inline::line)
            .collect();
        let together =
            figures.iter().map(line_width).sum::<usize>() + figures.len().saturating_sub(1) * 3;
        if together > self.room() {
            return self.marked(node, "- ", style::dim());
        }
        self.title(node);
        let mut row = vec![];
        for figure in figures {
            if !row.is_empty() {
                row.push(Span::styled(" · ", style::dim()));
            }
            row.extend(figure.spans);
        }
        self.put(&Line::from(row));
    }

    /// A form's list, every item behind the same marker.
    pub(super) fn marked(&mut self, node: Node<'_>, marker: &str, colour: ratatui::style::Style) {
        self.title(node);
        if let Some(list) = node.first() {
            self.items(list, |_| (marker.to_owned(), colour));
        }
    }

    /// A terminal folds nothing: the section under its title, set in.
    pub(super) fn details(&mut self, node: Node<'_>) {
        let title = if node.title().is_empty() {
            "Details"
        } else {
            node.title()
        };
        self.put(&Line::from(Span::styled(
            format!("▾ {}", printable(title)),
            style::bold(),
        )));
        self.within(Span::raw("  "), Span::raw("  "), |out| {
            out.blocks(node, Under::Blocks)
        });
    }

    pub(super) fn sources(&mut self, node: Node<'_>) {
        self.title(node);
        if let Some(list) = node.first() {
            self.items(list, |index| (format!("[{}] ", index + 1), style::dim()));
        }
    }
}
