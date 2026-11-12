//! The forms of a reply, drawn for a terminal: a callout behind a bar of its colour, steps
//! numbered down a rail, tabs one under another each under its label, figures side by side, each
//! number over what it counts (a list where no row holds them), the case for and against by
//! their markers, a section under its title, sources by number, each address on its own row.

use markdown::{Callout, Kind, Node};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::{Renderer, Under, inline};
use crate::printable::printable;
use crate::style;
use crate::wrap::{NO_BREAK, line_width, width};

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

    /// Each step its number, a rail down to the next one. The numbers end in one column and
    /// the rail runs under it: 9 and 10 are one chain.
    pub(super) fn steps(&mut self, node: Node<'_>) {
        self.title(node);
        let Some(list) = node.first() else { return };
        let (start, count) = (u64::from(list.start().max(1)), list.children().count());
        let wide = (start + count as u64).saturating_sub(1).to_string().len();
        for (index, item) in list.children().enumerate() {
            if index > 0 {
                self.gap = None;
                let mut joint = self.rest.clone();
                joint
                    .spans
                    .push(Span::styled(format!("{:>wide$}", "│"), style::dim()));
                self.emit(joint);
            }
            let number = format!("{:>wide$}  ", start + index as u64);
            let rest = if index + 1 == count {
                Span::raw(" ".repeat(wide + 2))
            } else {
                Span::styled(format!("{:>wide$}  ", "│"), style::dim())
            };
            // A step's name has a row of its own, where the step opens with one.
            self.step = item
                .first()
                .is_some_and(|first| matches!(first.kind(), Kind::Paragraph));
            self.within(
                Span::styled(number, style::done().patch(style::bold())),
                rest,
                |out| out.blocks(item, Under::Item),
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
            let rule = "─".repeat(self.room().min(40).saturating_sub(width(&label) + 3));
            self.put(&Line::from(vec![
                Span::styled("─ ", style::dim()),
                Span::styled(label, style::bold()),
                Span::styled(format!(" {rule}"), style::dim()),
            ]));
            self.blocks(tab, Under::Blocks);
        }
    }

    /// The figures side by side, each number over what it counts, while a row holds them; a
    /// list of them when it does not.
    pub(super) fn stats(&mut self, node: Node<'_>) {
        let Some(list) = node.first() else { return };
        let figures: Option<Vec<Figure>> = list.children().map(figure).collect();
        let Some(figures) = figures else {
            return self.marked(node, "• ", style::dim());
        };
        let wide = |figure: &Figure| line_width(&figure.0).max(line_width(&figure.1));
        let widths: Vec<usize> = figures.iter().map(wide).collect();
        let together = widths.iter().sum::<usize>() + figures.len().saturating_sub(1) * FIGURE_GAP;
        if together > self.room() {
            return self.marked(node, "• ", style::dim());
        }
        self.title(node);
        let (values, labels): (Vec<_>, Vec<_>) = figures.into_iter().unzip();
        for cells in [values, labels] {
            let mut row = Line::default();
            let mut owed = 0;
            for (cell, wide) in cells.into_iter().zip(&widths) {
                let own = line_width(&cell);
                // The space up to a figure is drawn only when one follows: no row ends in spaces.
                if own > 0 {
                    if owed > 0 {
                        row.spans.push(Span::raw(" ".repeat(owed)));
                    }
                    owed = 0;
                    // Code's spaces are joints only where words wrap; here they are spaces.
                    row.spans.extend(cell.spans.into_iter().map(|mut span| {
                        if span.content.contains(NO_BREAK) {
                            span.content = span.content.replace(NO_BREAK, " ").into();
                        }
                        span
                    }));
                }
                owed += wide - own + FIGURE_GAP;
            }
            if !row.spans.is_empty() {
                self.exact(row);
            }
        }
    }

    /// A form's list, every item behind the same marker.
    pub(super) fn marked(&mut self, node: Node<'_>, marker: &str, colour: Style) {
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

    /// Each source by its number: what it is called and what is said of it on a row, its address
    /// on the next, where it is whole whatever the name's length.
    pub(super) fn sources(&mut self, node: Node<'_>) {
        self.title(node);
        let Some(list) = node.first() else { return };
        let loose = matches!(list.kind(), Kind::List { tight: false, .. });
        for (index, item) in list.children().enumerate() {
            if index > 0 && loose {
                self.blank();
            }
            let marker = format!("[{}] ", index + 1);
            let pad = Span::raw(" ".repeat(width(&marker)));
            self.within(Span::styled(marker, style::dim()), pad, |out| {
                out.source(item);
            });
        }
    }

    fn source(&mut self, item: Node<'_>) {
        let link = only_text(item)
            .and_then(Node::first)
            .filter(|link| matches!(link.kind(), Kind::Link) && !link.dest().is_empty());
        // Not the link and its note this form is written as: the item is drawn as it is.
        let Some(link) = link else {
            return self.blocks(item, Under::Item);
        };
        let mut lines = vec![vec![]];
        let underlined = Style::new().add_modifier(Modifier::UNDERLINED);
        inline::each(link.children(), underlined, &mut lines);
        let name: String = lines.concat().iter().map(|span| &*span.content).collect();
        let (text, rest) = inline::parted(link.next());
        // What is said of a source is set off from its name, which a colour alone would not do.
        if !text.is_empty() || rest.is_some() {
            inline::push(&mut lines, " — ", style::dim());
            inline::push(&mut lines, text, style::dim());
            inline::each(inline::from(rest), style::dim(), &mut lines);
        }
        for line in lines {
            self.put(&Line::from(line));
        }
        let address = printable(link.dest());
        if !name.contains(&*address) {
            self.put(&Line::from(Span::styled(address, style::dim())));
        }
    }
}

/// A figure: its number, and under it what it counts.
type Figure = (Line<'static>, Line<'static>);

/// The columns between two figures.
const FIGURE_GAP: usize = 3;

/// An item as a figure, when it is one paragraph: its bold lead the number, the rest what it
/// counts; with no lead, all of it the number.
fn figure(item: Node<'_>) -> Option<Figure> {
    let text = only_text(item)?;
    let Some(lead) = inline::lead(text) else {
        return Some((inline::line(text), Line::default()));
    };
    let mut value = vec![vec![]];
    inline::one(lead.title, Style::new(), &mut value);
    let mut label = vec![vec![]];
    if let Some(when) = lead.meta {
        inline::one(when, style::dim(), &mut label);
        inline::push(&mut label, " ", style::dim());
    }
    inline::push(&mut label, lead.text, style::dim());
    inline::each(inline::from(lead.rest), style::dim(), &mut label);
    Some((inline::trimmed(&value), inline::trimmed(&label)))
}
