//! The questions the lanes put to the person, one open at a time: the others wait their turn in
//! the order they came. A lane that is not shown is named on its approvals and questions, and
//! they ignore keys for a moment as they come up (what was typed for the composer must not answer
//! them); its own choices (carry out the plan? `/login`'s steps) wait until it is shown.

use std::time::{Duration, Instant};

use super::app::{Action, App, Open};
use super::event::Out;
use super::questions;
use super::select::{Picked, Select};

/// How long a question ignores keys as it comes up.
const SETTLE: Duration = Duration::from_millis(300);

/// A question open or waiting its turn, and the lane that asked it.
pub(crate) struct Asked {
    pub(crate) select: Select,
    pub(crate) open: Open,
    /// The lane that asked: Ctrl-C stops it, and its `Dismiss` closes the question.
    pub(crate) asker: String,
}

impl App {
    /// Whether the session at `at` waits for the person: a question of its own is open or
    /// waiting its turn, or a choice of its own is held until it is shown.
    pub(crate) fn waits_for_person(&self, at: usize) -> bool {
        let view = &self.views.sessions[at];
        let held = view.lane.as_ref().is_some_and(|lane| !lane.held.is_empty());
        held || self.questions.iter().any(|asked| asked.asker == view.id)
    }

    /// The name lane `lane` is shown by on its questions while it is not shown; `None` when it is.
    pub(super) fn label(&self, lane: usize) -> Option<String> {
        (lane != self.views.home()).then(|| self.views.sessions[lane].name.clone())
    }

    /// A question from the lane at `lane`: it waits behind those already asked. It ignores keys
    /// at first when its lane is not shown, and an approval does even from the lane shown.
    pub(super) fn ask(&mut self, lane: usize, select: Select, open: Open, now: Instant) {
        let settles = matches!(open, Open::Approval(_)) || lane != self.views.home();
        let asker = self.views.sessions[lane].id.clone();
        self.questions.push_back(Asked {
            select,
            open,
            asker,
        });
        if self.questions.len() == 1 {
            self.settling = settles.then(|| now + SETTLE);
        }
    }

    /// A choice of the lane's own: asked now when it is shown, else held until it is.
    pub(super) fn ask_own(&mut self, lane: usize, select: Select, open: Open, now: Instant) {
        // The person left: nobody would answer it (held, it would never even be shown).
        if self.left {
            self.answered("", open, Picked::Cancelled);
            return;
        }
        if lane == self.views.home() {
            return self.ask(lane, select, open, now);
        }
        if let Some(state) = self.views.sessions[lane].lane.as_deref_mut() {
            state.held.push((select, open));
        }
    }

    /// The lane shown now asks what it held back, and its questions no longer name it.
    pub(super) fn release_held(&mut self) {
        let now = Instant::now();
        let home = self.views.home();
        let id = &self.views.sessions[home].id;
        for asked in self.questions.iter_mut().filter(|q| q.asker == *id) {
            asked.select.unlabel();
        }
        let Some(state) = self.views.sessions[home].lane.as_deref_mut() else {
            return;
        };
        for (select, open) in std::mem::take(&mut state.held) {
            self.ask(home, select, open, now);
        }
    }

    /// The question open takes `picked`: its answer goes to the lane that asked, and the next
    /// question comes up, ignoring keys for a moment (they were typed for this one).
    pub(super) fn answer_open(&mut self, picked: Picked, now: Instant) -> Option<String> {
        let asked = self.questions.pop_front()?;
        self.answered(&asked.asker, asked.open, picked);
        self.settling = (!self.questions.is_empty()).then(|| now + SETTLE);
        Some(asked.asker)
    }

    /// `open` is answered with `picked`: the reply goes to the server; what the person said
    /// refusing an approval waits for the call refused, in the asker's lane.
    fn answered(&mut self, asker: &str, open: Open, picked: Picked) {
        let (reply, said) = questions::answer(&open, picked);
        if let Some((id, reply)) = reply {
            self.outbox.push(Out::Reply(id, reply));
        }
        let lane = self.views.find(asker).map(|at| self.views.lane_of(at));
        if let Some(state) = lane.and_then(|at| self.views.sessions[at].lane.as_deref_mut())
            && said.is_some()
        {
            state.declined = said;
        }
    }

    /// The server withdrew question `id` (its task stopped, or it was answered elsewhere): it
    /// closes, open, waiting its turn or held, and nothing is replied.
    pub(super) fn resolved(&mut self, id: u64, now: Instant) {
        if let Some(found) = self.questions.iter().position(|q| q.open.id() == Some(id)) {
            self.questions.remove(found);
            if found == 0 {
                self.settling = (!self.questions.is_empty()).then(|| now + SETTLE);
            }
            return;
        }
        for view in &mut self.views.sessions {
            if let Some(state) = view.lane.as_deref_mut() {
                state.held.retain(|(_, open)| open.id() != Some(id));
            }
        }
    }

    /// What is shown may be incomplete (the server no longer kept all since the client's
    /// cursor): every transcript, the jobs and the questions start again from what follows.
    pub(super) fn gap(&mut self) {
        for view in &mut self.views.sessions {
            view.blocks.clear();
            view.running.clear();
            view.busy = None;
            if let Some(state) = view.lane.as_deref_mut() {
                state.held.clear();
            }
        }
        self.views.jobs.clear();
        self.questions.clear();
        self.settling = None;
        self.pending.clear();
        self.views.replay = true;
    }

    /// A key while a question is open: Ctrl-C says no and stops the lane that asked.
    pub(super) fn question_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        now: Instant,
    ) -> Option<Action> {
        use crossterm::event::{KeyCode, KeyModifiers};
        let stop = key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        if !stop && self.settling.is_some_and(|until| now < until) {
            return None;
        }
        let front = self.questions.front_mut()?;
        let picked = front.select.key(key)?;
        if matches!(front.open, Open::Quit) {
            return self.quit_answered(&picked, now);
        }
        let asker = self.answer_open(picked, now)?;
        // A lane with no task is not stopped: that would quit llx, every lane's work with it.
        let at = self
            .views
            .find(&asker)
            .filter(|&at| stop && self.views.sessions[at].busy.is_some())?;
        if let Some(state) = self.views.sessions[at].lane.as_deref_mut() {
            state.stopping = true;
        }
        Some(Action::Stop(asker))
    }

    /// Every question, open, waiting or held, closes unanswered: the person is leaving.
    pub(super) fn close_questions(&mut self) {
        let mut open: Vec<Open> = (std::mem::take(&mut self.questions).into_iter())
            .map(|asked| asked.open)
            .collect();
        for view in &mut self.views.sessions {
            let held = view
                .lane
                .as_deref_mut()
                .map(|l| std::mem::take(&mut l.held));
            open.extend(held.unwrap_or_default().into_iter().map(|(_, open)| open));
        }
        for open in open {
            self.answered("", open, Picked::Cancelled);
        }
    }
}
