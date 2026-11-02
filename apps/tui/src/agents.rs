//! How the sessions' events reach their views (`views.rs`): each session of the interface (a
//! lane) and the agents they started. Only the one shown goes to the terminal; showing
//! another clears the screen and its scrollback and draws that one's transcript from the start,
//! so what scrolls up is always one session's. The list's keys are in `list.rs`.

use std::time::Instant;

use protocol::Event;
use ratatui::text::{Line, Span};

use super::app::{Action, App};
use super::cells;
use super::screen::Block;
use super::style;
use super::views::{AgentState, Running, SessionView};
use crate::printable::printable;

mod ends;

impl App {
    /// What happened in the session at `at`: its transcript grows, and so does the screen when it
    /// is the one shown. What is not a session's own goes to the interface as a whole.
    pub(super) fn on(&mut self, at: usize, event: Event, now: Instant) -> Option<Action> {
        let width = self.width;
        let event = super::here::event(event, self.files.root());
        let view = &mut self.views.sessions[at];
        let block: Block = match event {
            Event::Header(facts) => {
                let (version, facts) = facts
                    .split_first()
                    .map_or(("", &[][..]), |(v, f)| (v.as_str(), f));
                cells::header(version, facts, width).into()
            }
            // In an agent, a message its parent sent it while it worked, read at its next step.
            Event::Note(text) if view.parent.is_some() => cells::user(&text, width).into(),
            Event::Note(text) => cells::note(&text, width).into(),
            Event::Error(text) => cells::error(&text, width).into(),
            Event::TaskStarted(goal) => view.started(&goal, now, width)?.into(),
            Event::Call { call, name, label } => {
                view.running.push(Running {
                    call,
                    name,
                    label,
                    since: now,
                });
                if let Some(lane) = &mut view.lane {
                    lane.steps.clear();
                }
                return None;
            }
            Event::Result {
                call,
                name,
                label,
                outcome,
            } => self.call_ended(at, &call, &name, &label, &outcome, now),
            Event::Answer(text) if !text.trim().is_empty() => cells::agent(&text, width).into(),
            Event::Answer(_) => return None,
            Event::TaskEnded {
                status,
                millis,
                turns,
                cost,
            } => self.task_ended(at, status, millis, turns, cost).into(),
            Event::Spent(tokens) => {
                view.tokens += tokens;
                return None;
            }
            Event::Started { id, name } => {
                let parent = Some(view.id.clone());
                if self.views.find(&id).is_none() {
                    let agent = SessionView::new(id, parent, name);
                    self.views.sessions.push(agent);
                }
                return None;
            }
            Event::Earlier(turns) => {
                view.tasks += turns.len();
                if let Some(last) = turns.last() {
                    view.state = AgentState::Ended(last.status.clone());
                }
                for block in cells::earlier(&turns, width) {
                    self.push(at, block);
                }
                return None;
            }
            Event::Updated(update) => {
                if let Some(title) = update.title {
                    view.name = title;
                }
                return None;
            }
            Event::Closed => {
                view.state = AgentState::Closed;
                view.running.clear();
                view.busy = None;
                return None;
            }
            Event::AgentsCleared => {
                self.agents_cleared(at);
                return None;
            }
            Event::LaneEnded => {
                self.lane_ended(at);
                return None;
            }
            event @ (Event::Mode(_)
            | Event::Step(_)
            | Event::LaneStarted { .. }
            | Event::Saved(_)
            | Event::Show(_)
            | Event::Total(_)
            | Event::Prefill(_)
            | Event::Context(_)
            | Event::Jobs(_)
            | Event::Queue { .. }
            | Event::Steered(_)
            | Event::Copy(_)
            | Event::Quit) => return self.lane_event(at, event),
        };
        self.push(at, block);
        None
    }

    /// `block` ends the transcript of the session at `at`, and is printed now when it is shown.
    pub(super) fn push(&mut self, at: usize, block: impl Into<Block>) {
        let block = block.into();
        let view = &mut self.views.sessions[at];
        if view.id == self.views.shown {
            self.pending.push(block.clone());
        }
        view.blocks.push(block);
    }

    /// The lane at `at` let its agents go: their views go too.
    fn agents_cleared(&mut self, at: usize) {
        let lane = self.views.sessions[at].id.clone();
        // Out of one of its agents' views; another lane shown stays shown.
        if self.views.home() == at {
            self.show(&lane);
        }
        let theirs = |session: &SessionView| session.parent.as_ref() == Some(&lane);
        self.views.sessions.retain(|session| !theirs(session));
    }

    /// Show the session `id`: its whole transcript, from the top. Another lane's gives the
    /// list's keys back to the prompt: the rows it chose among were the other lane's.
    pub(crate) fn show(&mut self, id: &str) {
        let Some(at) = self.views.find(id) else {
            return;
        };
        if id == self.views.shown {
            return;
        }
        if self.views.lane_of(at) != self.views.home() {
            self.views.focus = None;
        }
        self.views.shown = id.to_string();
        self.views.replay = true;
        self.views.sessions[at].unseen = false;
        self.release_held();
        let view = &self.views.sessions[at];
        self.pending = match &view.parent {
            None => view.blocks.clone(),
            Some(parent) => {
                let parent = self.views.find(parent).map(|at| &self.views.sessions[at]);
                let parent = parent.map_or("", SessionView::title);
                let mut blocks = vec![heading(&view.name, parent).into()];
                blocks.extend(view.blocks.iter().cloned());
                blocks
            }
        };
    }

    /// Whether the screen must be cleared before the next frame: the view changed.
    pub(crate) fn take_replay(&mut self) -> bool {
        std::mem::take(&mut self.views.replay)
    }

    /// The session shown.
    pub(crate) fn shown(&self) -> &SessionView {
        &self.views.sessions[self.views.find(&self.views.shown).unwrap_or(0)]
    }

    /// Whether a session works: the spinner turns.
    pub(crate) fn moving(&self) -> bool {
        let mut sessions = self.views.sessions.iter();
        sessions.any(|view| view.busy.is_some() || !view.running.is_empty())
    }
}

/// The first block of an agent's view: which agent it is, and the session that started it.
fn heading(name: &str, parent: &str) -> Vec<Line<'static>> {
    let mut line = vec![
        Span::styled("◆ ", style::code()),
        Span::styled(format!("agent {}", printable(name)), style::bold()),
    ];
    if !parent.is_empty() {
        let by = format!("  ·  started by {}", printable(parent));
        line.push(Span::styled(by, style::dim()));
    }
    vec![Line::from(line)]
}

#[cfg(test)]
#[path = "agents_tests.rs"]
mod tests;
