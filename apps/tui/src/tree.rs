//! The sessions under the composer: the lane shown and the agents it started (none while it has
//! started none), a row each with what it does now or how its last task ended, how long it has
//! worked and the tokens it used, and the jobs. The agents hang under their lane as a branch, in a
//! name column of their own. The other lanes are one line; the ← list has them. While the list
//! has the keys (↓ from an empty prompt), `❯` marks the row chosen.
//!
//! ```text
//!   ⠹ fix the parser  Messaging agent tests · run the suite · 2m 03s · 81.2k tokens
//! ❯   ├ ⠹ scout  Reading src/lib.rs · 1m 12s · 54.1k tokens
//!     └ ✓ tests  done · 48s · 20.3k tokens
//!   enter open · x stop · esc back
//! ```
//!
//! With no agents and no jobs, only the line for the other lanes, if there are any:
//!
//! ```text
//!   ← 2 other sessions (1 working, 1 waiting for you)
//! ```

use std::fmt::Write as _;
use std::time::Instant;

use ratatui::text::{Line, Span};

use super::app::App;
use super::cells;
use super::list::{dimmed, more};
use super::printable::printable;
use super::style;
use super::time::now_ms;
use super::wrap::width as text_width;

mod row;

use row::{Branch, Row, Weight, row, session_row};
pub(super) use row::{columns, spent};

/// Agents' rows shown at most, and jobs'; the rest are counted.
pub(super) const ROWS: usize = 5;
/// Columns a lane's name takes at most, here and in the ← list.
pub(super) const NAME: usize = 36;
/// Columns an agent's name takes at most, and a job's.
const AGENT: usize = 20;
/// A lane whose question waits for the person, open or held until it is shown.
const WAITS: &str = "waiting for you";

/// The list's rows: the lane shown with its agents, and the jobs; the other lanes are one line.
/// None while there is nothing of the kind.
pub(crate) fn render(
    app: &App,
    now: Instant,
    spinner: &dyn Fn(Instant) -> Span<'static>,
) -> Vec<Line<'static>> {
    let sessions = &app.views.sessions;
    let order = app.views.order();
    let others = others(app);
    if order.is_empty() && app.views.jobs.is_empty() {
        return match others {
            Some(others) => vec![dimmed(&others, app.width)],
            None => vec![],
        };
    }
    let width = app.width.max(10);
    let column = |widest: usize, most: usize| widest.clamp(1, most.min(width / 2));
    // An agent's name column is its siblings', and the last of them closes the branch.
    let names = (order.iter().skip(1)).map(|&at| text_width(&printable(sessions[at].title())));
    let agents = column(names.max().unwrap_or(0), AGENT);
    let focus = app.views.focus;
    let shown = order
        .iter()
        .position(|&at| sessions[at].id == app.views.shown);
    let line = |at_row: usize| {
        let view = &sessions[order[at_row]];
        let waits = app.waits_for_person(order[at_row]);
        let chosen = focus == Some(at_row);
        let mut row = session_row(view, chosen, shown == Some(at_row), now, spinner);
        if waits {
            (row.glyph, row.detail) = (Span::styled("?", style::waiting()), WAITS.into());
        }
        // The lane is the one on the screen anyway; an agent shown is marked.
        let agent = row.branch == Branch::Mid;
        if agent && row.weight == Weight::Shown {
            row.detail = format!("current · {}", row.detail);
        }
        let names = if agent {
            if at_row + 1 == order.len() {
                row.branch = Branch::Last;
            }
            agents
        } else {
            column(text_width(&printable(view.title())), NAME)
        };
        self::row(row, names, width)
    };
    let mut out = vec![];
    if !order.is_empty() {
        out.push(line(0));
        // A window of the agents around the row chosen, else the one shown.
        let rest = order.len() - 1;
        let around = focus
            .or(shown)
            .and_then(|row| row.checked_sub(1))
            .unwrap_or(0);
        let start = around
            .saturating_sub(ROWS - 1)
            .min(rest.saturating_sub(ROWS));
        let end = (start + ROWS).min(rest);
        if start > 0 {
            out.push(more(start, "↑"));
        }
        out.extend((start..end).map(|at| line(at + 1)));
        if end < rest {
            out.push(more(rest - end, "↓"));
        }
    }
    let names = (app.views.jobs.iter()).map(|(_, job)| text_width(&printable(&job.name)));
    let names = column(names.max().unwrap_or(0), AGENT);
    out.extend(jobs(app, order.len(), names, width));
    let on_agent = app.shown().parent.is_some();
    let others = others.unwrap_or_else(|| "← sessions".into());
    let hint = match (focus, on_agent) {
        (Some(_), _) => "enter open · x stop · esc back".to_string(),
        (None, true) => format!("↓ manage · esc back to {}", app.home_title()),
        (None, false) => format!("↓ manage · {others}"),
    };
    out.push(dimmed(&hint, width));
    out
}

/// The lanes not shown, counted, with those that need a look: `← 2 other sessions (1 working)`.
fn others(app: &App) -> Option<String> {
    let home = app.views.home();
    let sessions = &app.views.sessions;
    let lanes: Vec<usize> = (0..sessions.len())
        .filter(|&at| at != home && sessions[at].lane.is_some())
        .collect();
    if lanes.is_empty() {
        return None;
    }
    let count = |test: &dyn Fn(usize) -> bool| lanes.iter().filter(|&&at| test(at)).count();
    let working = count(&|at| sessions[at].busy.is_some());
    let waiting = count(&|at| app.waits_for_person(at));
    let finished = count(&|at| sessions[at].unseen && sessions[at].busy.is_none());
    let notes: Vec<String> = [
        (working, "working"),
        (waiting, WAITS),
        (finished, "finished"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, what)| format!("{n} {what}"))
    .collect();
    let mut said = format!("← {}", cells::count(lanes.len() as u64, "other session"));
    if !notes.is_empty() {
        let _ = write!(said, " ({})", notes.join(", "));
    }
    Some(said)
}

/// The jobs' rows: a monitor (`≋`, its command, how much is left) or a schedule (`◷`, when).
fn jobs(app: &App, first: usize, names: usize, width: usize) -> Vec<Line<'static>> {
    let mut out = vec![];
    for (at, (_, job)) in app.views.jobs.iter().enumerate().take(ROWS) {
        let (glyph, right) = match (job.until_at, &job.next) {
            (Some(until), _) => {
                let left = until.saturating_sub(now_ms()) / 1000;
                let events = cells::count(job.events as u64, "event");
                let right = format!("{events} · {} left", cells::clock(left));
                (Span::styled("≋", style::code()), right)
            }
            (None, next) => {
                let when = next.as_deref().unwrap_or_default();
                (Span::styled("◷", style::waiting()), format!("next {when}"))
            }
        };
        out.push(row(
            Row {
                chosen: app.views.focus == Some(first + at),
                branch: Branch::None,
                glyph,
                name: &job.name,
                detail: job.detail.clone(),
                right,
                weight: Weight::Plain,
            },
            names,
            width,
        ));
    }
    if app.views.jobs.len() > ROWS {
        out.push(more(app.views.jobs.len() - ROWS, "↓"));
    }
    out
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
