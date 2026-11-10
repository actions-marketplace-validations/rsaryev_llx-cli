//! One row of the tree: a session's, by what it does or how it ended and what it spent, or a
//! job's; a pointer, a branch, a glyph, a name column and what is said after it.

use std::time::Instant;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::cells::{self, tokens};
use crate::printable::printable;
use crate::style;
use crate::time;
use crate::tool_cell;
use crate::views::{AgentState, Running, SessionView};
use crate::wrap::{cut, width as text_width};

/// A session's row: a lane by what its task does or how its last one ended, an agent by its
/// state too (indented under its lane); on the right, how long it worked and its tokens (a
/// lane's count its agents').
pub(super) fn session_row<'a>(
    view: &'a SessionView,
    chosen: bool,
    shown: bool,
    now: Instant,
    spinner: &dyn Fn(Instant) -> Span<'static>,
) -> Row<'a> {
    let (glyph, detail) = match (&view.parent, &view.state, view.busy) {
        (None, _, Some(since)) => (
            spinner(since),
            doing(&view.running).unwrap_or_else(|| "working".into()),
        ),
        (None, AgentState::Ended(status), None) => {
            let (glyph, word) = ended(status);
            let unseen = if view.unseen { " · unseen" } else { "" };
            (glyph, format!("{word}{unseen}"))
        }
        (None, _, None) => (Span::styled("●", style::code()), "idle".into()),
        (_, AgentState::Idle, _) => (Span::styled("○", style::dim()), "starting".to_string()),
        (_, AgentState::Working, busy) => (
            busy.map_or_else(|| Span::raw("○"), spinner),
            doing(&view.running).unwrap_or_else(|| "thinking".into()),
        ),
        (_, AgentState::Ended(status), _) => ended(status),
        (_, AgentState::Closed, _) => (Span::styled("○", style::dim()), "closed".into()),
    };
    Row {
        chosen,
        branch: if view.parent.is_some() {
            Branch::Mid
        } else {
            Branch::None
        },
        glyph,
        name: view.title(),
        detail,
        right: spent(view, now),
        weight: if shown {
            Weight::Shown
        } else if view.state == AgentState::Closed {
            Weight::Dimmed
        } else {
            Weight::Plain
        },
    }
}

/// What the calls running do: the first, and how many more.
fn doing(running: &[Running]) -> Option<String> {
    let first = tool_cell::doing(&running.first()?.label);
    Some(match running.len() {
        1 => first,
        n => format!("{first} +{} more", n - 1),
    })
}

/// How long a session has worked, its task running too, and the tokens it used: `1m 08s · 54.1k
/// tokens`; nothing while it has neither a second nor a token.
pub(crate) fn spent(view: &SessionView, now: Instant) -> String {
    let millis = view.worked + view.busy.map_or(0, |since| time::elapsed_ms(now, since));
    match (millis < 1000, view.tokens) {
        (true, 0) => String::new(),
        (_, 0) => cells::clock(millis / 1000),
        (_, count) => format!("{} · {} tokens", cells::clock(millis / 1000), tokens(count)),
    }
}

/// Where a row hangs: an agent's under its lane, the last of its lane's closing the branch.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Branch {
    None,
    Mid,
    Last,
}

/// How a row's name is drawn: the session on the screen, a closed one, a command that failed or
/// was stopped, or any other.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Weight {
    Shown,
    Dimmed,
    Failed,
    Plain,
}

pub(super) struct Row<'a> {
    /// The row the list's keys are on.
    pub(super) chosen: bool,
    pub(super) branch: Branch,
    pub(super) glyph: Span<'static>,
    pub(super) name: &'a str,
    pub(super) detail: String,
    /// Time and tokens, after what it does.
    pub(super) right: String,
    pub(super) weight: Weight,
}

pub(super) fn row(row: Row, names: usize, width: usize) -> Line<'static> {
    // No column kept for a pointer: only the row the keys are on has one.
    let pointer = match row.chosen {
        true => Span::styled("❯ ", style::code()),
        false => Span::raw(""),
    };
    let name_style = match row.weight {
        Weight::Shown => style::bold(),
        Weight::Dimmed => style::dim(),
        Weight::Failed => style::failed(),
        Weight::Plain => Style::default(),
    };
    let branch = match row.branch {
        Branch::None => "",
        Branch::Mid => "├ ",
        Branch::Last => "└ ",
    };
    let taken = text_width(&pointer.content) + text_width(branch);
    let room = width.saturating_sub(taken);
    let right = printable(&row.right);
    // Time and tokens, or how a job runs, end at the right edge, as the footer's spend does; a
    // row too narrow for both says them after what it does.
    let beside = names + 4 + MIN_DETAIL + 2 + text_width(&right);
    let (said, right) = match !right.is_empty() && beside > room {
        true => (format!("{} · {right}", row.detail), String::new()),
        false => (row.detail, right),
    };
    let left = match right.is_empty() {
        true => room,
        false => room.saturating_sub(text_width(&right) + 2),
    };
    let mut spans = vec![pointer, Span::styled(branch, style::dim())];
    spans.extend(columns(
        (row.glyph, row.name, name_style),
        &said,
        (names, left),
    ));
    if !right.is_empty() {
        let drawn: usize = spans.iter().map(|span| text_width(&span.content)).sum();
        let gap = width.saturating_sub(drawn + text_width(&right));
        spans.push(Span::raw(" ".repeat(gap)));
        spans.push(Span::styled(right, style::dim()));
    }
    Line::from(spans)
}

/// The fewest columns of what a row does kept beside its right side.
const MIN_DETAIL: usize = 12;

/// A row after its pointer: a mark, the name in its column, and where it stands, dimmed. `room`
/// is what is left of the width after the pointer (and a branch).
pub(crate) fn columns(
    (glyph, name, name_style): (Span<'static>, &str, Style),
    said: &str,
    (names, room): (usize, usize),
) -> Vec<Span<'static>> {
    let name = cut(&printable(name), names);
    let pad = names.saturating_sub(text_width(&name));
    let said = printable(&said.replace('\n', " "));
    vec![
        glyph,
        Span::raw(" "),
        Span::styled(name, name_style),
        Span::raw(" ".repeat(pad + 2)),
        Span::styled(cut(&said, room.saturating_sub(names + 4)), style::dim()),
    ]
}

/// How a session's last task ended, as a mark and a word.
fn ended(status: &str) -> (Span<'static>, String) {
    let (style, mark, word, _) = cells::ending(status);
    (Span::styled(mark, style), word.into())
}
