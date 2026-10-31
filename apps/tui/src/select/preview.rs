//! The marked option's preview (a mockup, a snippet), in a box beside the options, its detail
//! under it; under the options instead where the terminal leaves too little room beside them.
//!
//! ```text
//! ❯ 1. Sidebar      ┌────────────┐
//!   2. Top nav      │ +--+-----+ │
//!                   │ +--+-----+ │
//!                   └────────────┘
//!                   Nav on the left
//! ```

use ratatui::text::{Line, Span};

use super::Select;
use crate::printable::printable;
use crate::style;
use crate::wrap::{cut, line_width, width as text_width, wrap};

/// Between the options and the box.
const GAP: usize = 2;
/// The least room a box takes beside the options; with less, it goes under them.
const NARROWEST: usize = 24;
/// The most rows of a preview shown; the rest are counted.
const MOST_ROWS: usize = 20;

impl Select {
    /// Whether the options have previews: they are drawn with them, and their details go under.
    pub(super) fn previews(&self) -> bool {
        self.choices.iter().any(|choice| !choice.preview.is_empty())
    }

    /// The options `rows`, with the marked one's preview beside them, or under them.
    pub(super) fn beside(&self, rows: &[usize], width: usize) -> Vec<Line<'static>> {
        let natural = rows.iter().map(|at| self.natural(*at)).max().unwrap_or(0);
        let left = natural.min(width / 2);
        let room = width.saturating_sub(left + GAP);
        let marked = self.choices.get(self.at);
        let (preview, detail) =
            marked.map_or(("", ""), |c| (c.preview.as_str(), c.detail.as_str()));
        if room < NARROWEST {
            let mut out: Vec<Line<'static>> = (rows.iter())
                .flat_map(|at| self.row(*at, (0, 0), width))
                .collect();
            let under = shown(preview, detail, width.saturating_sub(GAP));
            if !under.is_empty() {
                out.push(Line::default());
            }
            let indent = Span::raw(" ".repeat(GAP));
            out.extend(under.into_iter().map(|mut line| {
                line.spans.insert(0, indent.clone());
                line
            }));
            return out;
        }
        let options: Vec<Line<'static>> = (rows.iter())
            .flat_map(|at| self.row(*at, (0, 0), left))
            .collect();
        let right = shown(preview, detail, room);
        let height = options.len().max(right.len());
        (0..height)
            .map(|n| {
                let mut line = options.get(n).cloned().unwrap_or_default();
                if let Some(beside) = right.get(n) {
                    let pad = left.saturating_sub(line_width(&line)) + GAP;
                    line.spans.push(Span::raw(" ".repeat(pad)));
                    line.spans.extend(beside.spans.iter().cloned());
                }
                line
            })
            .collect()
    }

    /// How wide option `at` is on one row: its mark, number, check and label.
    fn natural(&self, at: usize) -> usize {
        let choice = &self.choices[at];
        let number = format!("{}. ", at + 1).len();
        let check = match self.multiple && !choice.types {
            true => 4,
            false => 0,
        };
        2 + number + check + text_width(&printable(&choice.label))
    }
}

/// A preview boxed within `width` columns, then the detail under it, dim; nothing for neither.
fn shown(preview: &str, detail: &str, width: usize) -> Vec<Line<'static>> {
    let mut out = boxed(preview, width);
    if !detail.is_empty() {
        let line = Line::from(Span::styled(printable(detail), style::dim()));
        out.extend(wrap(&line, width, &Line::default(), &Line::default()));
    }
    out
}

/// `preview`'s lines in a box at most `width` wide: each made printable (a tab as spaces), a line
/// too long cut with `…`, past [`MOST_ROWS`] the rest counted.
fn boxed(preview: &str, width: usize) -> Vec<Line<'static>> {
    if preview.is_empty() {
        return vec![];
    }
    let mut lines: Vec<String> = (preview.lines())
        .map(|line| printable(&line.replace('\t', "    ")))
        .collect();
    if lines.len() > MOST_ROWS {
        let more = lines.len() - (MOST_ROWS - 1);
        lines.truncate(MOST_ROWS - 1);
        lines.push(format!("… {more} more lines"));
    }
    let most = width.saturating_sub(4).max(1);
    let lines: Vec<String> = lines.iter().map(|line| cut(line, most)).collect();
    let inner = lines.iter().map(|line| text_width(line)).max().unwrap_or(0);
    let edge = "─".repeat(inner + 2);
    let mut out = vec![Line::from(Span::styled(format!("┌{edge}┐"), style::dim()))];
    for line in lines {
        let pad = " ".repeat(inner - text_width(&line));
        out.push(Line::from(vec![
            Span::styled("│ ", style::dim()),
            Span::raw(format!("{line}{pad}")),
            Span::styled(" │", style::dim()),
        ]));
    }
    out.push(Line::from(Span::styled(format!("└{edge}┘"), style::dim())));
    out
}
