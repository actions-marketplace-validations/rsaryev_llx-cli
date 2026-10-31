//! The sessions in this project, as ← on an empty prompt lists them: the lanes open (working,
//! then waiting for the person, then idle or done), a new session, then this directory's saved
//! sessions, newest first. ↑↓ choose, Enter or → opens (a lane is shown, a saved session opens
//! in a lane of its own), `x` stops a lane's task without switching to it, Esc or ← close it.
//!
//! Live and saved rows share their columns: a mark, the name, then where it stands.
//!
//! ```text
//! • Sessions in this project
//!
//! ❯ ⠹ fix the flaky jobs test   running · 1m 12s · 54.1k tokens · 2 agents
//!   ? add tabs to the printer   waiting for you
//!   ● refactor the picker       current · idle · 3 tasks
//!   + New session
//!
//!   Earlier
//!     make the release binary   2 h ago · 4 tasks
//!
//!   enter open · x stop · esc back
//! ```

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::app::{Action, App};
use super::cells;
use super::list::{stepped, window_start};
use super::questions::heading;
use super::style;
use super::tree::NAME;
use super::views::AgentState;
use protocol::{Saved, Target};

/// Rows shown at once; a longer list scrolls with the row chosen.
const WINDOW: usize = 14;
#[derive(Debug, Clone, PartialEq)]
enum Row {
    Live(String),
    New,
    /// A saved session: its id, its first task, and when it ran and its tasks.
    Saved(Saved),
}

/// The list open, and the row chosen.
pub(super) struct SessionList {
    rows: Vec<Row>,
    at: usize,
}

impl App {
    /// Whether ← on the composer opens the list: it is empty and offers nothing.
    pub(super) fn opens_sessions(&self, key: &KeyEvent) -> bool {
        key.code == KeyCode::Left
            && key.modifiers.is_empty()
            && self.composer.is_empty()
            && self.offers().is_empty()
    }

    /// Open the list on the newest lane not shown, else on "New session".
    pub(super) fn open_sessions(&mut self) {
        let rows = self.session_rows();
        let home = &self.views.sessions[self.views.home()].id;
        let newest = (self.views.sessions.iter().rev())
            .find(|view| view.lane.is_some() && view.id != *home)
            .map(|view| Row::Live(view.id.clone()));
        let at = (rows.iter().position(|row| Some(row) == newest.as_ref()))
            .or_else(|| rows.iter().position(|row| *row == Row::New));
        self.listing = Some(SessionList {
            rows,
            at: at.unwrap_or(0),
        });
    }

    /// The saved sessions came: the list open lists them, on the row it was on.
    pub(super) fn saved(&mut self, saved: Vec<Saved>) {
        self.saved = saved;
        let Some(list) = &self.listing else { return };
        let on = list.rows.get(list.at).cloned();
        let rows = self.session_rows();
        let at = rows.iter().position(|row| Some(row) == on.as_ref());
        self.listing = Some(SessionList {
            at: at.unwrap_or(0),
            rows,
        });
    }

    fn session_rows(&self) -> Vec<Row> {
        let views = &self.views.sessions;
        let mut live: Vec<usize> = (0..views.len())
            .filter(|&at| views[at].lane.is_some())
            .collect();
        let rank = |at: usize| {
            if views[at].busy.is_some() {
                0
            } else if self.waits_for_person(at) {
                1
            } else {
                2
            }
        };
        live.sort_by_key(|&at| rank(at));
        let mut rows: Vec<Row> = live
            .iter()
            .map(|&at| Row::Live(views[at].id.clone()))
            .collect();
        rows.push(Row::New);
        let saved = self
            .saved
            .iter()
            .filter(|saved| self.views.find(&saved.id).is_none());
        rows.extend(saved.cloned().map(Row::Saved));
        rows
    }

    /// A key while the list is open.
    pub(super) fn sessions_key(&mut self, key: KeyEvent, now: Instant) -> Option<Action> {
        let list = self.listing.as_mut()?;
        let last = list.rows.len().saturating_sub(1);
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "crossterm's keys, a foreign enum: the others do nothing here"
        )]
        match key.code {
            KeyCode::Up | KeyCode::Down => list.at = stepped(list.at, last, key.code),
            KeyCode::Esc | KeyCode::Left => self.listing = None,
            KeyCode::Enter | KeyCode::Right => {
                let row = list.rows.get(list.at).cloned();
                self.listing = None;
                match row? {
                    Row::Live(id) => self.show(&id),
                    Row::New => self.opening(Target::New),
                    Row::Saved(saved) => self.opening(Target::Saved(saved.id)),
                }
            }
            KeyCode::Char('x') if key.modifiers.is_empty() => {
                let Some(Row::Live(id)) = list.rows.get(list.at).cloned() else {
                    return None;
                };
                self.halt_lane(&id);
            }
            // Ctrl-C closes the list, and does nothing more.
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.listing = None;
            }
            // Ctrl-D and the like are the interface's: the list closes first.
            _ if !key.modifiers.is_empty() => {
                self.listing = None;
                return self.key(key, now);
            }
            _ => {}
        }
        None
    }

    /// The list's rows at `now`.
    pub(crate) fn sessions_render(
        &self,
        now: Instant,
        spinner: &dyn Fn(Instant) -> Span<'static>,
    ) -> Vec<Line<'static>> {
        let Some(list) = &self.listing else {
            return vec![];
        };
        let width = self.width.max(20);
        let mut out = vec![heading("Sessions in this project", ""), Line::default()];
        let names = (list.rows.iter())
            .filter_map(|row| match row {
                Row::Live(id) => self
                    .views
                    .find(id)
                    .map(|at| self.views.sessions[at].title()),
                Row::Saved(saved) => Some(saved.title.as_str()),
                Row::New => None,
            })
            .map(super::wrap::width)
            .max()
            .unwrap_or(0)
            .clamp(12, NAME.min(width / 2));
        let start = window_start(list.at, list.rows.len(), WINDOW);
        let mut earlier = false;
        for (at, row) in list.rows.iter().enumerate().skip(start).take(WINDOW) {
            if matches!(row, Row::Saved(..)) && !std::mem::replace(&mut earlier, true) {
                out.push(Line::default());
                out.push(Line::from(Span::styled("  Earlier", style::dim())));
            }
            let pointer = match at == list.at {
                true => Span::styled("❯ ", style::bold()),
                false => Span::raw("  "),
            };
            let mut spans = vec![pointer];
            match row {
                Row::Live(id) => spans.extend(self.live_row(id, names, now, spinner)),
                Row::New => spans.push(Span::styled("+ New session", style::bold())),
                Row::Saved(saved) => spans.extend(super::tree::columns(
                    (Span::raw(" "), &saved.title, Style::default()),
                    &saved.meta,
                    (names, width.saturating_sub(2)),
                )),
            }
            out.push(Line::from(spans));
        }
        out.push(Line::default());
        let hint = "  enter open · x stop · esc back";
        out.push(Line::from(Span::styled(hint, style::dim())));
        out
    }

    /// A lane's row: its mark, its name, and where it stands.
    fn live_row(
        &self,
        id: &str,
        names: usize,
        now: Instant,
        spinner: &dyn Fn(Instant) -> Span<'static>,
    ) -> Vec<Span<'static>> {
        let Some(at) = self.views.find(id) else {
            return vec![];
        };
        let view = &self.views.sessions[at];
        let agents = (self.views.sessions.iter())
            .filter(|agent| agent.parent.as_deref() == Some(id))
            .count();
        let spent = Some(super::tree::spent(view, now)).filter(|text| !text.is_empty());
        let mut said = vec![];
        let glyph = match (&view.state, view.busy) {
            (_, Some(since)) => {
                said.push("running".into());
                said.extend(spent);
                spinner(since)
            }
            _ if self.waits_for_person(at) => {
                said.push("waiting for you".into());
                Span::styled("?", style::waiting())
            }
            (AgentState::Ended(status), None) => {
                let (style, mark, word, _) = cells::ending(status);
                said.push(word.into());
                if view.unseen {
                    said.push("unseen".into());
                }
                said.extend(spent);
                Span::styled(mark, style)
            }
            _ => {
                said.push("idle".into());
                Span::styled("●", style::code())
            }
        };
        if at == self.views.home() {
            said.insert(0, "current".into());
        }
        if view.tasks > 0 && view.busy.is_none() {
            said.push(cells::count(view.tasks as u64, "task"));
        }
        if agents > 0 {
            said.push(cells::count(agents as u64, "agent"));
        }
        let said = said.join(" · ");
        super::tree::columns(
            (glyph, view.title(), style::bold()),
            &said,
            (names, self.width.saturating_sub(2)),
        )
    }
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
