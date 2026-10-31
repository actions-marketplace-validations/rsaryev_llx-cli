//! Styled Markdown table cells, laid out within the terminal's display columns.

use pulldown_cmark::Alignment;
use ratatui::text::{Line, Span};

use crate::style;
use crate::wrap::{line_width, wrap};

pub(super) struct Table {
    pub(super) rows: Vec<Vec<Line<'static>>>,
    alignments: Vec<Alignment>,
}

impl Table {
    pub(super) fn new(alignments: Vec<Alignment>) -> Self {
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
        let mut widths = vec![1; columns];
        for row in &self.rows {
            for (at, cell) in row.iter().enumerate().take(columns) {
                widths[at] = widths[at].max(line_width(cell));
            }
        }
        let budget = room.saturating_sub(columns * 3 + 1);
        while widths.iter().sum::<usize>() > budget {
            let widest = widths.iter().enumerate().max_by_key(|(_, width)| **width);
            let Some((at, _)) = widest else { break };
            widths[at] -= 1;
        }
        for (index, mut row) in self.rows.into_iter().enumerate() {
            row.resize_with(columns, Line::default);
            let mut cells: Vec<Vec<Line<'static>>> = row
                .iter()
                .zip(&widths)
                .map(|(cell, width)| wrap(cell, *width, &Line::default(), &Line::default()))
                .collect();
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
                        Alignment::Right => padding,
                        Alignment::Center => padding / 2,
                        Alignment::None | Alignment::Left => 0,
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
            if index == 0 {
                let rule = widths
                    .iter()
                    .map(|width| "─".repeat(width + 2))
                    .collect::<Vec<_>>()
                    .join("┼");
                let mut line = rest.clone();
                line.spans
                    .push(Span::styled(format!("├{rule}┤"), style::dim()));
                out.push(line);
            }
        }
        out
    }
}
