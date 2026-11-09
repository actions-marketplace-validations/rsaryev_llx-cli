//! The blocks of a session's history, other than tool calls ([`super::tool_cell`]): what the
//! person asked, what the model answered, how a task ended, and what went wrong. Each is a pure
//! function of its content and the width; the screen puts one blank row between blocks.
//!
//! ```text
//! ❯ make a landing page for a phone shop
//!
//! The page is in site/index.html.
//!
//! ─ Worked for 48s · 6 turns · $0.0087 ──────────────────────
//! ```

use std::fmt::Write as _;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::markdown;
use super::screen::Block;
use super::style;
use super::wrap::{cut, line_width, width as text_width, wrap};
use crate::printable::printable;

/// What the person typed.
pub(crate) fn user(text: &str, width: usize) -> Vec<Line<'static>> {
    let first = Line::from(Span::styled("❯ ", style::bold()));
    let rest = Line::default();
    text.trim()
        .lines()
        .enumerate()
        .flat_map(|(at, line)| {
            let line = Line::from(Span::styled(printable(line), style::bold()));
            wrap(&line, width, if at == 0 { &first } else { &rest }, &rest)
        })
        .collect()
}

/// What the model wrote, as markdown, flush with the left edge: no bullet and no indent, so a
/// copied answer is its text alone.
pub(crate) fn agent(text: &str, width: usize) -> Vec<Line<'static>> {
    markdown::render(text.trim(), width)
}

/// How a task ended.
pub(crate) struct End<'a> {
    /// As the run reports it: `done`, `no_data`, `exhausted`, `error`, `interrupted`.
    pub status: &'a str,
    pub millis: u64,
    pub turns: u64,
    pub cost: f64,
}

/// A rule across the width with how long the task took and what it cost.
pub(crate) fn end(end: &End, width: usize) -> Vec<Line<'static>> {
    let verb = match end.status {
        "interrupted" => "Stopped after",
        "error" => "Failed after",
        // Its turns, its time or its budget ran out: the error line under it says which.
        "exhausted" => "Ran out after",
        _ => "Worked for",
    };
    let turns = count(end.turns, "turn");
    let text = format!(
        "─ {verb} {} · {turns} · ${:.4} ",
        duration(end.millis),
        end.cost
    );
    let fill = width.saturating_sub(text_width(&text));
    vec![Line::from(Span::styled(
        format!("{text}{}", "─".repeat(fill)),
        style::dim(),
    ))]
}

/// Tasks shown at most of a resumed session; the earlier ones are counted.
const EARLIER: usize = 20;

/// A resumed session's tasks as blocks: how many there were, then each task, its answer, and how
/// it ended when it did not end well. Only the tasks and answers are kept, not their calls.
pub(crate) fn earlier(turns: &[protocol::Turn], width: usize) -> Vec<Block> {
    let skipped = turns.len().saturating_sub(EARLIER);
    let mut said = count(turns.len() as u64, "earlier task");
    if skipped > 0 {
        let _ = write!(said, " · the last {EARLIER} shown");
    }
    let mut out = vec![Block::drawn(width, move |width| note(&said, width))];
    for turn in &turns[skipped..] {
        let goal = turn.goal.clone();
        out.push(Block::drawn(width, move |width| user(&goal, width)));
        if !turn.answer.trim().is_empty() {
            let answer = turn.answer.clone();
            out.push(Block::drawn(width, move |width| agent(&answer, width)));
        }
        if turn.status != "done" {
            let (style, mark, word, _) = ending(&turn.status);
            let line = Line::from(vec![
                Span::styled(format!("{mark} "), style),
                Span::styled(word, style::dim()),
            ]);
            out.push(vec![line].into());
        }
    }
    out
}

/// Something that went wrong outside a tool call: the model, the network, the session.
pub(crate) fn error(text: &str, width: usize) -> Vec<Line<'static>> {
    let line = Line::from(Span::styled(printable(text.trim()), style::failed()));
    wrap(
        &line,
        width,
        &Line::from(Span::styled("■ ", style::failed())),
        &Line::default(),
    )
}

/// A quiet line from llx itself: a setting changed, a session resumed.
/// Its rows are kept as they are (a table, a list of commands), and only a row too wide wraps.
pub(crate) fn note(text: &str, width: usize) -> Vec<Line<'static>> {
    let mut out = vec![];
    for row in text.trim_matches('\n').lines() {
        let row = printable(row.trim_end());
        match text_width(&row) <= width {
            true => out.push(Line::from(Span::styled(row, style::dim()))),
            false => {
                // A table's row (a label, two spaces or more, then its text) wraps under its
                // second column, so the columns stay; any other row wraps flush.
                let trimmed = row.trim_start();
                let label = (trimmed.find("  ").and_then(|at| trimmed.get(at..)))
                    .map(|rest| row.len() - rest.trim_start().len())
                    .filter(|&end| {
                        (row.get(..end)).is_some_and(|head| text_width(head) * 2 < width)
                    })
                    .unwrap_or(0);
                let (label, body) = row.split_at(label);
                let under = " ".repeat(text_width(label));
                let first = Line::from(Span::styled(label.to_string(), style::dim()));
                let body = Line::from(Span::styled(body.to_string(), style::dim()));
                out.extend(wrap(&body, width, &first, &Line::from(under)));
            }
        }
    }
    out
}

/// The header of the server's `Event::Header`: its version first, then its facts.
pub(crate) fn header_of(said: &[String], width: usize) -> Vec<Line<'static>> {
    let (version, facts) = (said.split_first()).map_or(("", &[][..]), |(v, f)| (v.as_str(), f));
    header(version, facts, width)
}

/// The session's first row: the version, then what it runs with.
/// It stays one row: the last fact (the folder) loses its start when the row is too narrow.
pub(crate) fn header(version: &str, facts: &[impl AsRef<str>], width: usize) -> Vec<Line<'static>> {
    let mut line = Line::from(vec![Span::styled(format!("llx {version}"), style::bold())]);
    let facts: Vec<String> = (facts.iter().map(AsRef::as_ref))
        .filter(|f| !f.is_empty())
        .map(printable)
        .collect();
    let count = facts.len();
    for (at, mut fact) in facts.into_iter().enumerate() {
        if at + 1 == count {
            let room = width.saturating_sub(line_width(&line) + 3);
            fact = cut_start(&fact, room);
        }
        line.spans
            .push(Span::styled(format!(" · {fact}"), style::dim()));
    }
    vec![line]
}

/// `text` without its start when wider than `columns`: `…/scratchpad/ws1`.
fn cut_start(text: &str, columns: usize) -> String {
    if text_width(text) <= columns {
        return text.to_string();
    }
    let reversed: String = text.chars().rev().collect();
    cut(&reversed, columns).chars().rev().collect()
}

/// `48s`, `1m 05s`, `1h 02m`.
pub(crate) fn duration(millis: u64) -> String {
    match millis {
        // Where the tenth would round up to ten seconds, the clock says them.
        0..9_950 => format!("{:.1}s", millis as f64 / 1000.0),
        _ => clock(millis.max(10_000) / 1000),
    }
}

/// `n` of `noun`: `1 turn`, `3 turns`.
pub(crate) fn count(n: u64, noun: &str) -> String {
    match n {
        1 => format!("1 {noun}"),
        n => format!("{n} {noun}s"),
    }
}

/// `950`, `54.1k`, `1.2M`.
pub(super) fn tokens(count: u64) -> String {
    match count {
        0..1000 => count.to_string(),
        // From 999,950 the tenth rounds up to `1000.0k`: a million, said as one.
        1000..999_950 => format!("{:.1}k", count as f64 / 1000.0),
        _ => format!("{:.1}M", count as f64 / 1_000_000.0),
    }
}

/// How a task ended, as the interface says it: a style, a mark and a word for its row (`✓
/// done`), and what a line about it says it did (`finished`).
pub(crate) fn ending(status: &str) -> (Style, &'static str, &'static str, &'static str) {
    match status {
        "done" => (style::done(), "✓", "done", "finished"),
        "no_data" => (style::failed(), "✗", "no data", "finished with no data"),
        // Its turns, its time or its budget: the status does not say which.
        "exhausted" => (style::failed(), "✗", "limit reached", "reached a limit"),
        "interrupted" => (style::waiting(), "■", "stopped", "was stopped"),
        _ => (style::failed(), "✗", "failed", "failed"),
    }
}

/// Whole `seconds` as they count up: `12s`, `3m 08s`, `1h 02m`.
pub(crate) fn clock(seconds: u64) -> String {
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m {:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h {:02}m", seconds / 3600, seconds / 60 % 60),
    }
}

#[cfg(test)]
#[path = "cells_tests.rs"]
mod tests;
