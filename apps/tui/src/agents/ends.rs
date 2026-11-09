//! What the end of a call and of a task adds to a session's transcript, and tells the others.

use std::time::{Duration, Instant};

use protocol::{Label, Outcome};
use ratatui::text::{Line, Span};

use super::AgentState;
use crate::app::App;
use crate::cells::{self, End};
use crate::printable::printable;
use crate::screen::Block;
use crate::style;
use crate::tool_cell::{self, State};

/// A call that took this long says so in its row.
const TOOK: Duration = Duration::from_millis(500);

/// A finished call as a block: the person's refusal when the call failed on it (the gate's
/// words), which uses up what they said; otherwise its outcome, and what they said waits for the
/// call they refused (calls of several sessions finish in any order).
/// How long it took (`took`), where that is worth a glance, ends its first row.
fn result_block(
    declined: &mut Option<String>,
    call: (&str, &Label, &Outcome),
    took: Option<String>,
    width: usize,
) -> Block {
    let (name, label, outcome) = call;
    let timed = move |mut lines: Vec<Line<'static>>| {
        if let (Some(took), Some(head)) = (&took, lines.first_mut()) {
            head.spans
                .push(Span::styled(format!(" · {took}"), style::dim()));
        }
        lines
    };
    let label = label.clone();
    match (declined.take(), outcome) {
        (Some(said), Outcome::Failed { declined: true, .. }) => Block::drawn(width, move |width| {
            timed(tool_cell::declined(&label, &said, width))
        }),
        (said, _) => {
            *declined = said;
            let chain = tool_cell::chain(name, &label, outcome);
            let (name, outcome) = (name.to_string(), outcome.clone());
            Block::drawn(width, move |width| {
                timed(tool_cell::lines(
                    &name,
                    &label,
                    &State::Done(&outcome),
                    width,
                ))
            })
            .chained(chain)
        }
    }
}

impl App {
    /// The lane at `at` ended: it goes, another shown in its place; not as the person quits,
    /// when every lane ends and the screen stays as they left it.
    pub(super) fn lane_ended(&mut self, at: usize) {
        let id = self.views.sessions[at].id.clone();
        if let Some(next) = (!self.left).then(|| self.views.lane_ended(&id)).flatten() {
            self.show(&next);
        }
    }

    /// A call of the session at `at` ended: its row, with how long it took.
    pub(super) fn call_ended(
        &mut self,
        at: usize,
        call: &str,
        name: &str,
        label: &Label,
        outcome: &Outcome,
        now: Instant,
    ) -> Block {
        let width = self.width;
        let view = &mut self.views.sessions[at];
        let running = view.running.iter().position(|running| running.call == call);
        let started = running.map(|at| view.running.remove(at).since);
        let took = started.map(|started| now.saturating_duration_since(started));
        if let Some(lane) = &mut view.lane {
            lane.steps.clear();
        }
        let lane = self.views.lane_of(at);
        let lane = self.views.sessions[lane].lane.as_deref_mut();
        let mut none = None;
        let declined = lane.map_or(&mut none, |lane| &mut lane.declined);
        let took = (took.filter(|took| *took >= TOOK))
            .map(|took| cells::duration(u64::try_from(took.as_millis()).unwrap_or(u64::MAX)));
        result_block(declined, (name, label, outcome), took, width)
    }

    /// The task of the session at `at` ended: its last row, and a line where it is not seen.
    pub(super) fn task_ended(
        &mut self,
        at: usize,
        status: String,
        millis: u64,
        turns: u64,
        cost: f64,
    ) -> Block {
        let width = self.width;
        let view = &mut self.views.sessions[at];
        view.busy = None;
        view.running.clear();
        view.worked += millis;
        if let Some(lane) = &mut view.lane {
            lane.stopping = false;
        }
        view.tasks += 1;
        let ended = status.clone();
        let block = Block::drawn(width, move |width| {
            let end = End {
                status: &ended,
                millis,
                turns,
                cost,
            };
            cells::end(&end, width)
        });
        // Its lane hears of an agent's end too, and the session shown of a lane's that
        // is not.
        let parent = view.parent.clone().and_then(|id| self.views.find(&id));
        let name = self.views.sessions[at].title().to_string();
        if let Some(parent) = parent {
            let line = finished_line("Agent", &name, &status, millis, "");
            self.push(parent, vec![line]);
        } else if at != self.views.home() {
            self.views.sessions[at].unseen = true;
            let line = finished_line("Session", &name, &status, millis, " · ← to open");
            let shown = self.views.shown_at();
            self.push(shown, vec![line]);
        }
        let view = &mut self.views.sessions[at];
        if view.state != AgentState::Closed {
            view.state = AgentState::Ended(status);
        }
        block
    }
}

/// `● Agent "scout" finished · 3m 38s` in its lane's transcript, and `● Session "…" finished ·
/// 3m 08s · ← to open` in the one shown; `after` ends it.
fn finished_line(kind: &str, name: &str, status: &str, millis: u64, after: &str) -> Line<'static> {
    let (dot, _, _, verb) = cells::ending(status);
    let took = cells::clock(millis / 1000);
    Line::from(vec![
        Span::styled("● ", dot),
        Span::raw(format!("{kind} \"{}\" {verb}", printable(name))),
        Span::styled(format!(" · {took}{after}"), style::dim()),
    ])
}
