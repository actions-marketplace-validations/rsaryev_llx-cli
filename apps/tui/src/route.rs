//! Where each of the server's events goes in the interface: a session's own (`agents.rs`), then
//! what only a lane has or is the interface's as a whole (a lane started, the sessions listed, a
//! lane shown, the end). The server's own events go as the lane shown's. Each stage names what it
//! passes on, so an event the protocol gains is placed in one of them, never dropped unseen (S10).

use std::time::Instant;

use super::app::{Action, App};
use super::cells;
use protocol::Event;

/// Rows of the latest steps shown under their call.
const STEPS: usize = 3;

impl App {
    /// What happened in session `id` (a lane or an agent); one no longer shown is let go.
    pub(crate) fn event_in(&mut self, id: &str, event: Event, now: Instant) -> Option<Action> {
        let at = self.views.find(id)?;
        self.routed(at, event, now)
    }

    /// What happened in the server as a whole, or in the lane shown.
    pub(crate) fn event(&mut self, event: Event, now: Instant) -> Option<Action> {
        self.routed(self.views.home(), event, now)
    }

    /// The event from the session at `at` taken; the list's choice stays on what it names as
    /// rows come and go around it, and goes with its own row.
    fn routed(&mut self, at: usize, event: Event, now: Instant) -> Option<Action> {
        let action = self.on(at, event, now);
        self.views.settle();
        action
    }

    /// What only a lane has, and what is the interface's as a whole (a lane started, the
    /// sessions listed, a lane shown, the jobs, the composer's words, a quit). An agent's view
    /// has no lane: its mode, steps and counts go unheard. `at` is the session the event came
    /// from.
    pub(super) fn lane_event(&mut self, at: usize, event: Event) -> Option<Action> {
        match (event, self.views.sessions[at].lane.as_deref_mut()) {
            (Event::Mode(mode), Some(lane)) => lane.mode = mode,
            (Event::Step(step), Some(lane)) => {
                lane.steps.push(step);
                let over = lane.steps.len().saturating_sub(STEPS);
                lane.steps.drain(..over);
            }
            (Event::Context(left), Some(lane)) => lane.context = Some(left),
            (Event::Total(total), Some(lane)) => lane.total = Some(total),
            (
                Event::Queue {
                    ready,
                    steered,
                    queued,
                    background,
                },
                Some(lane),
            ) => {
                (lane.ready, lane.steered, lane.queued, lane.background) =
                    (ready, steered, queued, background)
            }
            // The task read what the person said to it: theirs now, in the transcript.
            (Event::Steered(texts), Some(_)) => {
                for text in texts {
                    let block = cells::user(&text, self.width);
                    self.push(at, block);
                }
            }
            (
                Event::Mode(_)
                | Event::Step(_)
                | Event::Context(_)
                | Event::Total(_)
                | Event::Queue { .. }
                | Event::Steered(_),
                None,
            ) => {}
            // Each session's jobs, in the order of the sessions.
            (Event::Jobs(rows), _) => {
                let lane = self.views.lane_of(at);
                let id = self.views.sessions[lane].id.clone();
                self.views.jobs.retain(|(lane, _)| *lane != id);
                self.views
                    .jobs
                    .extend(rows.into_iter().map(|row| (id.clone(), row)));
                let mut jobs = std::mem::take(&mut self.views.jobs);
                jobs.sort_by_key(|(lane, _)| self.views.find(lane));
                self.views.jobs = jobs;
            }
            (Event::LaneStarted { id, name }, _) => {
                self.views.lane_started(id.clone(), &name);
                if let (Some(said), Some(at)) = (self.closed.take(), self.views.find(&id)) {
                    self.push(at, cells::note(&said, self.width));
                }
            }
            (Event::Show(id), _) => self.show(&id),
            (Event::Saved(rows), _) => self.saved(rows),
            (Event::Prefill(text), _) => self.composer.set(&text),
            (Event::Copy(text), _) => return Some(Action::Copy(text)),
            (Event::Quit, _) => return Some(Action::Quit),
            // A session's own events, which [`App::on`] takes.
            (
                Event::Header(_)
                | Event::Note(_)
                | Event::Error(_)
                | Event::TaskStarted(_)
                | Event::Call { .. }
                | Event::Result { .. }
                | Event::Answer(_)
                | Event::TaskEnded { .. }
                | Event::Earlier(_)
                | Event::Updated(_)
                | Event::LaneEnded
                | Event::Started { .. }
                | Event::Closed
                | Event::Spent(_)
                | Event::AgentsCleared,
                _,
            ) => {}
        }
        None
    }
}
