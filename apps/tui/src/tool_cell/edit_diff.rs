//! A tool's unified edit diff as numbered, coloured rows, never clipped log output.

use ratatui::text::{Line, Span};

use crate::printable::printable;
use crate::style;
use crate::wrap::{fit, hard, width as columns};

/// `diff`'s hunks as rows of at most `width`: each line under its old and new numbers, its sign
/// and code coloured, a long one broken under its own start; `…` between two hunks.
pub(super) fn render(diff: &str, width: usize) -> Vec<Line<'static>> {
    let (mut old, mut new) = (0, 0);
    let mut out: Vec<Line<'static>> = vec![];
    for line in diff.lines() {
        if let Some(header) = line.strip_prefix("@@ -") {
            if !out.is_empty() {
                out.push(Line::from(Span::styled("    …", style::dim())));
            }
            let mut parts = header.split_whitespace();
            old = number(parts.next().unwrap_or_default());
            new = number(parts.next().unwrap_or_default().trim_start_matches('+'));
            continue;
        }
        let (before, after) = (old, new);
        let (mark, colour) = match line.as_bytes().first() {
            Some(b'-') => {
                old += 1;
                ('-', style::failed())
            }
            Some(b'+') => {
                new += 1;
                ('+', style::done())
            }
            Some(b' ') => {
                old += 1;
                new += 1;
                (' ', style::dim())
            }
            _ => {
                for text in hard(&printable(line), width.saturating_sub(4).max(1)) {
                    out.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(text, style::dim()),
                    ]));
                }
                continue;
            }
        };
        // A removed line has no new number, an added one no old.
        let shown = |on: bool, n: usize| if on { n.to_string() } else { String::new() };
        // Only the sign and the code carry the colour: the numbers stay dim, nothing is banded.
        let mut numbers = format!(
            "    {:>3} {:>3} ",
            shown(mark != '+', before),
            shown(mark != '-', after)
        );
        let mut sign = format!("{mark} ");
        let indent = columns(&numbers) + columns(&sign);
        let room = width.saturating_sub(indent).max(1);
        let text = line.get(1..).unwrap_or_default();
        for (at, text) in hard(&printable(text), room).into_iter().enumerate() {
            let row = if at == 0 {
                Line::from(vec![
                    Span::styled(std::mem::take(&mut numbers), style::dim()),
                    Span::styled(std::mem::take(&mut sign), colour),
                    Span::styled(text, colour),
                ])
            } else {
                Line::from(vec![
                    Span::raw(" ".repeat(indent)),
                    Span::styled(text, colour),
                ])
            };
            // Extremely narrow terminals still retain the signs and line numbers.
            out.extend(fit(row, width));
        }
    }
    out
}

fn number(range: &str) -> usize {
    range
        .split(',')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}
