//! A long choice in tabs: its rows in consecutive groups (a provider's models), the rows of one
//! listed at a time. Tab and Shift+Tab go round them, and each keeps its marker; a search counts
//! each tab's matches, and goes to the tab of the best one when the tab shown has none.
//!
//! ```text
//!   OpenAI 36 · [OpenRouter 288] · Anthropic 15
//! ```

use std::ops::Range;

use ratatui::text::{Line, Span};

use crate::printable::printable;
use crate::style;
use crate::wrap::wrap;

pub(super) struct Tabs {
    names: Vec<String>,
    /// Each tab's rows, as positions in the whole list.
    bounds: Vec<Range<usize>>,
    /// The tab shown.
    at: usize,
    /// Each tab's marker when it was left.
    marks: Vec<Option<usize>>,
}

impl Tabs {
    /// `groups` over a list of `rows`, shown on the tab of `marked`; `None` when no group has
    /// rows. One group is a tab too: it names whose the rows are. Rows past the groups are the
    /// last one's.
    pub(super) fn new(groups: &[protocol::Group], rows: usize, marked: usize) -> Option<Self> {
        let mut names = vec![];
        let mut bounds: Vec<Range<usize>> = vec![];
        let mut start = 0;
        for group in groups {
            let end = (start + group.rows).min(rows);
            if end > start {
                names.push(group.name.clone());
                bounds.push(start..end);
            }
            start = end;
        }
        bounds.last_mut()?.end = rows;
        let mut tabs = Self {
            marks: vec![None; names.len()],
            names,
            bounds,
            at: 0,
        };
        tabs.show(marked);
        Some(tabs)
    }

    /// Whether there is another tab to switch to.
    pub(super) fn several(&self) -> bool {
        self.bounds.len() > 1
    }

    /// Whether `row` is one of the tab shown.
    pub(super) fn holds(&self, row: usize) -> bool {
        self.bounds[self.at].contains(&row)
    }

    /// The tab of `row` shown, with `row` its marker.
    pub(super) fn show(&mut self, row: usize) {
        if let Some(at) = self.bounds.iter().position(|rows| rows.contains(&row)) {
            self.at = at;
            self.marks[at] = Some(row);
        }
    }

    /// The next tab shown (or the one before), the one left marked at `from`: the marker it had
    /// when it was left, if it was.
    pub(super) fn step(&mut self, forward: bool, from: usize) -> Option<usize> {
        self.marks[self.at] = Some(from);
        let count = self.bounds.len();
        self.at = match forward {
            true => (self.at + 1) % count,
            false => (self.at + count - 1) % count,
        };
        self.marks[self.at]
    }

    /// Each tab named with how many of `listed` are its rows, the one shown in brackets.
    pub(super) fn render(&self, listed: &[usize], width: usize) -> Vec<Line<'static>> {
        let mut line = Line::default();
        for (at, (name, rows)) in self.names.iter().zip(&self.bounds).enumerate() {
            if at > 0 {
                line.spans.push(Span::styled(" · ", style::dim()));
            }
            let count = listed.iter().filter(|row| rows.contains(row)).count();
            let name = printable(name);
            line.spans.push(match at == self.at {
                true => Span::styled(format!("[{name} {count}]"), style::bold()),
                false => Span::styled(format!("{name} {count}"), style::dim()),
            });
        }
        let indent = Line::from("  ");
        wrap(&line, width, &indent, &indent)
    }
}

#[cfg(test)]
#[path = "tabs_tests.rs"]
mod tests;
