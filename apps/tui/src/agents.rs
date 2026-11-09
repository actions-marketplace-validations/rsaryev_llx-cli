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
                Block::drawn(width, move |width| cells::header_of(&facts, width))
            }
            // In an agent, a message its parent sent it while it worked, read at its next step.
            Event::Note(text) if view.parent.is_some() => Block::text(width, text, cells::user),
            Event::Note(text) => Block::text(width, text, cells::note),
            Event::Error(text) => Block::text(width, text, cells::error),
            Event::TaskStarted(goal) => {
                view.started(&goal, now)?;
                Block::text(width, goal, cells::user)
            }
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
            Event::Answer(text) if !text.trim().is_empty() => {
                Block::text(width, text, cells::agent)
            }
            Event::Answer(_) => return None,
            Event::TaskEnded {
                status,
                millis,
                turns,
                cost,
            } => self.task_ended(at, status, millis, turns, cost),
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
            Event::Earlier(turns) => return self.earlier(at, &turns),
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
            | Event::JobOutput { .. }
            | Event::Queue { .. }
            | Event::Steered(_)
            | Event::Copy(_)
            | Event::Quit) => return self.lane_event(at, event),
        };
        self.push(at, block);
        None
    }

    /// The session at `at` was resumed: its earlier tasks, the last few drawn.
    fn earlier(&mut self, at: usize, turns: &[protocol::Turn]) -> Option<Action> {
        let view = &mut self.views.sessions[at];
        view.tasks += turns.len();
        if let Some(last) = turns.last() {
            view.state = AgentState::Ended(last.status.clone());
        }
        for block in cells::earlier(turns, self.width) {
            self.push(at, block);
        }
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
    /// list's keys back to the prompt: the rows it chose among were the other lane's. So does
    /// one shown in place of a lane that ended, though a job chosen is still drawn.
    pub(crate) fn show(&mut self, id: &str) {
        let Some(at) = self.views.find(id) else {
            return;
        };
        if id == self.views.shown {
            return;
        }
        // The lane shown until now, `None` when it just ended.
        let was = self.views.find(&self.views.shown);
        if was.map(|shown| self.views.lane_of(shown)) != Some(self.views.lane_of(at)) {
            self.views.focus = None;
        }
        self.views.shown = id.to_string();
        self.views.sessions[at].unseen = false;
        self.release_held();
        self.redraw();
    }

    /// The screen cleared and the session shown printed again from the top, its blocks fit to
    /// the width of the next frame.
    pub(crate) fn redraw(&mut self) {
        let Some(at) = self.views.find(&self.views.shown) else {
            return;
        };
        self.views.replay = true;
        let view = &self.views.sessions[at];
        let width = self.width;
        let mut blocks = vec![];
        if let Some(parent) = &view.parent {
            let parent = self.views.find(parent).map(|at| &self.views.sessions[at]);
            let parent = parent.map_or("", SessionView::title);
            blocks.push(heading(&view.name, parent).into());
        }
        blocks.extend(view.blocks.iter().map(|block| block.at(width)));
        self.pending = blocks;
    }

    /// Whether the screen must be cleared before the next frame: the view changed.
    pub(crate) fn take_replay(&mut self) -> bool {
        std::mem::take(&mut self.views.replay)
    }

    /// The session shown.
    pub(crate) fn shown(&self) -> &SessionView {
        &self.views.sessions[self.views.shown_at()]
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
