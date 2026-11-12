//! Styled Markdown table cells, laid out within the terminal's display columns: each column given
//! room by what it holds, and the rows ruled apart when they hold long text.

use markdown::Align;
use ratatui::text::{Line, Span};

use crate::style;
use crate::wrap::{line_width, width, wrap};

pub(super) struct Table {
    pub(super) rows: Vec<Vec<Line<'static>>>,
    alignments: Vec<Align>,
}

impl Table {
    pub(super) fn new(alignments: Vec<Align>) -> Self {
        Self {
            rows: vec![],
            alignments,
        }
    }

    pub(super) fn render(
        self,
        width: usize,
        first: &Line<'static>,
        rest: &Line<'static>,
    ) -> Vec<Line<'static>> {
        let columns = self.alignments.len();
        if columns == 0 {
            return vec![];
        }
        let room = width.saturating_sub(line_width(rest).max(line_width(first)));
        let mut out = vec![];
        // On tiny terminals preserve cells vertically rather than clipping their contents.
        if room < columns * 5 + 1 {
            for row in self.rows {
                if !out.is_empty() {
                    out.push(rest.clone());
                }
                for cell in row {
                    let prefix = if out.is_empty() { first } else { rest };
                    out.extend(wrap(&cell, width, prefix, rest));
                }
            }
            return out;
        }
        let widths = widths(&self.rows, columns, room.saturating_sub(columns * 3 + 1));
        let rule = widths
            .iter()
            .map(|width| "─".repeat(width + 2))
            .collect::<Vec<_>>()
            .join("┼");
        let mut rule_row = rest.clone();
        rule_row
            .spans
            .push(Span::styled(format!("├{rule}┤"), style::dim()));
        let mut laid: Vec<Vec<Vec<Line<'static>>>> = vec![];
        for mut row in self.rows {
            row.resize_with(columns, Line::default);
            let cells = row.iter().zip(&widths);
            let cells =
                cells.map(|(cell, width)| wrap(cell, *width, &Line::default(), &Line::default()));
            laid.push(cells.collect());
        }
        // Rows of long text are ruled apart, or no eye tells where one ends: when half of them
        // take three lines or more. Short cells that wrap once would only grow taller.
        let tall = |row: &&Vec<Vec<Line<'static>>>| row.iter().any(|cell| cell.len() >= 3);
        let body = laid.len().saturating_sub(1);
        let ruled = body > 0 && laid.iter().skip(1).filter(tall).count() * 2 >= body;
        let last = laid.len().saturating_sub(1);
        for (index, mut cells) in laid.into_iter().enumerate() {
            let height = cells.iter().map(Vec::len).max().unwrap_or(1);
            for line in 0..height {
                let mut rendered = if out.is_empty() {
                    first.clone()
                } else {
                    rest.clone()
                };
                rendered.spans.push(Span::styled("│", style::dim()));
                for at in 0..columns {
                    let cell = cells[at]
                        .get_mut(line)
                        .map(std::mem::take)
                        .unwrap_or_default();
                    let padding = widths[at].saturating_sub(line_width(&cell));
                    let left = match self.alignments[at] {
                        Align::Right => padding,
                        Align::Center => padding / 2,
                        Align::None | Align::Left => 0,
                    };
                    rendered.spans.push(Span::raw(" ".repeat(left + 1)));
                    rendered
                        .spans
                        .extend(cell.spans.into_iter().map(|mut span| {
                            if index == 0 {
                                span.style = span.style.patch(style::bold());
                            }
                            span
                        }));
                    rendered
                        .spans
                        .push(Span::raw(" ".repeat(padding - left + 1)));
                    rendered.spans.push(Span::styled("│", style::dim()));
                }
                out.push(rendered);
            }
            if index == 0 || (ruled && index < last) {
                out.push(rule_row.clone());
            }
        }
        out
    }
}

/// Each column's width within `budget`, as a browser lays a table out: what its widest cell takes
/// when all fit, else its longest word and a share of the rest by how much more it would take, so
/// a column of names stays narrow beside one of sentences.
fn widths(rows: &[Vec<Line<'static>>], columns: usize, budget: usize) -> Vec<usize> {
    let mut most = vec![1; columns];
    let mut least = vec![1; columns];
    for row in rows {
        for ((cell, most), least) in row.iter().zip(&mut most).zip(&mut least) {
            *most = (*most).max(line_width(cell));
            let words = cell.spans.iter().flat_map(|span| span.content.split(' '));
            *least = (*least).max(words.map(width).max().unwrap_or(0));
        }
    }
    if most.iter().sum::<usize>() <= budget {
        return most;
    }
    // No word claims more than an even share: a long address is broken, not the table.
    let share = (budget / columns).max(1);
    for least in &mut least {
        *least = (*least).min(share);
    }
    let spare = budget.saturating_sub(least.iter().sum());
    let more = |(most, least): (&usize, &usize)| most.saturating_sub(*least);
    let wanted: usize = most.iter().zip(&least).map(more).sum();
    let mut widths: Vec<usize> = most
        .iter()
        .zip(&least)
        .map(|pair| pair.1 + more(pair) * spare / wanted.max(1))
        .collect();
    // What the division left over goes to the columns still short, the first ones first.
    let mut left = budget.saturating_sub(widths.iter().sum());
    while left > 0 {
        let before = left;
        for (width, most) in widths.iter_mut().zip(&most) {
            if left > 0 && *width < *most {
                *width += 1;
                left -= 1;
            }
        }
        if left == before {
            break;
        }
    }
    widths
}
