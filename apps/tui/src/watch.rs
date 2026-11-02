//! A command in the background, open: what it printed so far, how it runs or how it ended, and
//! its keys. Enter on its row in the list opens it; while open, what it printed is asked of the
//! server each second (`Command::JobOutput`); `x` stops it, or lets one that ended leave the list;
//! Esc goes back. It closes as its row leaves the list.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use protocol::{Command, JobKind, JobRow};
use ratatui::text::{Line, Span};

use super::app::{Action, App};
use super::event::Out;
use super::printable::printable;
use super::style;
use super::time::now_ms;
use super::wrap::{cut, wrap};

/// How often what it printed is asked for while it is open.
const EVERY: Duration = Duration::from_secs(1);

/// The command open: its lane and job, and what it printed when last asked.
pub(crate) struct Watching {
    lane: String,
    job: String,
    text: String,
    asked: Option<Instant>,
}

impl App {
    /// Job `job` of lane `lane` opens.
    pub(crate) fn watch(&mut self, lane: String, job: String) {
        self.watching = Some(Watching {
            lane,
            job,
            text: String::new(),
            asked: None,
        });
    }

    /// While a command is open, what it printed is asked for, each second.
    pub(crate) fn poll(&mut self, now: Instant) {
        let Some(watching) = &mut self.watching else {
            return;
        };
        if watching
            .asked
            .is_some_and(|at| now.duration_since(at) < EVERY)
        {
            return;
        }
        watching.asked = Some(now);
        let (lane, job) = (watching.lane.clone(), watching.job.clone());
        self.outbox
            .push(Out::Call(Command::JobOutput { lane, job }));
    }

    /// What job `job` of lane `lane` printed so far, as the server said it.
    pub(crate) fn job_output(&mut self, lane: &str, job: &str, text: String) {
        if let Some(watching) = &mut self.watching
            && watching.lane == lane
            && watching.job == job
        {
            watching.text = text;
        }
    }

    /// The jobs changed: the command open closes when its row has left the list.
    pub(crate) fn jobs_changed(&mut self) {
        if self.watching.is_some() && self.watched().is_none() {
            self.watching = None;
        }
    }

    /// A key while a command is open.
    pub(crate) fn watch_key(&mut self, key: KeyEvent) -> Option<Action> {
        let ctrl_c =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        // Esc, ←, or Ctrl-C as it closes whatever is open, goes back; nothing stops.
        if ctrl_c || matches!(key.code, KeyCode::Esc | KeyCode::Left) {
            self.watching = None;
        } else if key.code == KeyCode::Char('x')
            && key.modifiers.is_empty()
            && let Some(watching) = &self.watching
        {
            let (lane, job) = (watching.lane.clone(), watching.job.clone());
            self.outbox.push(Out::Call(Command::StopJob { lane, job }));
        }
        None
    }

    /// The row of the command open, while it is listed.
    fn watched(&self) -> Option<&JobRow> {
        let watching = self.watching.as_ref()?;
        (self.views.jobs.iter())
            .find(|(lane, row)| *lane == watching.lane && row.id == watching.job)
            .map(|(_, row)| row)
    }
}

/// How a command in the background or a monitor runs or ended, as its row and its view say it,
/// styled: red when it failed or was stopped.
pub(crate) fn state(row: &JobRow) -> Span<'static> {
    let said = how(row);
    match &row.ended {
        Some(end) if end.failed => Span::styled(said, style::failed()),
        Some(_) | None => Span::styled(said, style::dim()),
    }
}

/// How it runs (`running · 4m 28s left`, a monitor's `3 checks · 4m 28s left`) or ended.
pub(crate) fn how(row: &JobRow) -> String {
    if let Some(end) = &row.ended {
        return printable(&end.how);
    }
    let left = row.until_at.unwrap_or_default().saturating_sub(now_ms()) / 1000;
    let left = super::cells::clock(left);
    match row.kind {
        JobKind::Monitor => {
            let checks = super::cells::count(row.events as u64, "check");
            format!("{checks} · {left} left")
        }
        JobKind::Shell | JobKind::Schedule => format!("running · {left} left"),
    }
}

/// The glyph a job's row and its view start with: how it ended, else what it is.
pub(crate) fn glyph(row: &JobRow) -> Span<'static> {
    match (&row.ended, row.kind) {
        (Some(end), _) if end.failed => Span::styled("✗", style::failed()),
        (Some(_), _) => Span::styled("✓", style::dim()),
        (None, JobKind::Monitor) => Span::styled("≋", style::code()),
        (None, JobKind::Shell) => Span::styled("$", style::code()),
        (None, JobKind::Schedule) => Span::styled("◷", style::waiting()),
    }
}

/// The command open, drawn in the place of the composer: its name and state, its command, the
/// end of what it printed (as many rows as the window leaves room for), and its keys.
pub(crate) fn render(app: &App, width: usize) -> Vec<Line<'static>> {
    let (Some(watching), Some(row)) = (&app.watching, app.watched()) else {
        return vec![];
    };
    let name = Span::styled(cut(&printable(&row.name), width / 2), style::bold());
    let mut out = vec![
        Line::from(vec![
            glyph(row),
            Span::raw(" "),
            name,
            Span::styled(" · ", style::dim()),
            state(row),
        ]),
        Line::from(Span::styled(
            format!(
                "  {}",
                cut(&printable(&row.detail), width.saturating_sub(2))
            ),
            style::dim(),
        )),
        Line::default(),
    ];
    let indent = Line::from("  ");
    let mut printed: Vec<Line<'static>> = (watching.text.lines())
        .flat_map(|line| wrap(&Line::from(printable(line)), width, &indent, &indent))
        .collect();
    if printed.is_empty() {
        let none = if watching.asked.is_some() {
            "(no output yet)"
        } else {
            ""
        };
        printed.push(Line::from(Span::styled(format!("  {none}"), style::dim())));
    }
    // The end of it, as a terminal shows a command running: the window's height less the rest.
    let room = app.height.saturating_sub(10).max(3);
    out.extend(printed.split_off(printed.len().saturating_sub(room)));
    out.push(Line::default());
    let keys = match row.ended {
        None => "x stop · esc back",
        Some(_) => "x remove · esc back",
    };
    out.push(Line::from(Span::styled(format!("  {keys}"), style::dim())));
    out
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
