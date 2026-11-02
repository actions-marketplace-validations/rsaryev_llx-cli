//! Stopping a task between model turns: when its time is up, when its budget is spent, when the
//! person stopped it (Ctrl-C), or when no one reads its output any more (noticed on a write: with
//! `--json`, every event). The chat model is wrapped, so the agent's loop ends the way it ends on
//! any unreachable model, after the tool call in flight has finished; nothing exits the process
//! from another thread. (A stopped task's running command is killed by the shell tool itself, and
//! the gate approves nothing more.)

use std::sync::Mutex;
use std::time::{Duration, Instant};

use contracts::chat::{Chat, ChatError, Completion, Request};

use crate::gate::STOPPED;
use crate::sync::lock;

/// Why a task was stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Halt {
    /// Its time limit ran out.
    TimedOut,
    /// The reader of its output went away.
    Closed,
    /// It spent its budget.
    OverBudget,
    /// The person stopped it.
    Interrupted,
}

/// What bounds a task here: its time, and the most it may spend. Its agents' requests go through
/// the same brake, so their spend counts.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Limits {
    pub time: Duration,
    /// Dollars; `None` is no bound.
    pub budget: Option<f64>,
}

impl Limits {
    /// Bounded in time alone.
    #[cfg(test)]
    pub(crate) fn time(time: Duration) -> Self {
        Self { time, budget: None }
    }
}

/// The chat model, stopped once the task's time is up, its budget spent, the person stopped it or
/// its output is closed.
pub(crate) struct Brake<'a> {
    chat: &'a dyn Chat,
    /// Whether the output's reader has gone.
    closed: &'a (dyn Fn() -> bool + Sync),
    /// Whether the person stopped the task.
    stopped: &'a (dyn Fn() -> bool + Sync),
    /// One lock for the task's bounds and why it stopped, so a check and its halt are one act
    /// (R7).
    armed: Mutex<Armed>,
}

/// A task's bounds, what it spent, and why it was stopped.
#[derive(Debug, Default)]
struct Armed {
    deadline: Option<Instant>,
    budget: Option<f64>,
    spent: f64,
    halt: Option<Halt>,
}

impl Armed {
    /// Whether the time is up or the budget spent, recorded as the halt.
    fn due(&mut self) -> Option<Halt> {
        let late = self.deadline.is_some_and(|at| Instant::now() >= at);
        let broke = self.budget.is_some_and(|most| self.spent >= most);
        let halt = match (late, broke) {
            (true, _) => Some(Halt::TimedOut),
            (false, true) => Some(Halt::OverBudget),
            (false, false) => None,
        };
        self.halt = halt.or(self.halt);
        halt
    }
}

impl<'a> Brake<'a> {
    pub(crate) fn new(
        chat: &'a dyn Chat,
        closed: &'a (dyn Fn() -> bool + Sync),
        stopped: &'a (dyn Fn() -> bool + Sync),
    ) -> Self {
        Self {
            chat,
            closed,
            stopped,
            armed: Mutex::default(),
        }
    }

    /// Start a task within `limits`; what it spent before is forgotten.
    pub(crate) fn arm(&self, limits: Limits) {
        *lock(&self.armed) = Armed {
            deadline: Instant::now().checked_add(limits.time),
            budget: limits.budget,
            spent: 0.0,
            halt: None,
        };
    }

    /// Move the deadline `by` later: the time spent waiting on the person (an approval, a
    /// question) is not the task's, as the time at the prompt line is not.
    pub(crate) fn extend(&self, by: Duration) {
        let mut armed = lock(&self.armed);
        if let Some(at) = armed.deadline {
            armed.deadline = Some(at.checked_add(by).unwrap_or(at));
        }
    }

    /// Whether the task's time is up or its budget spent, recorded as its halt: a task that
    /// waits on its monitors or agents makes no model turn to be stopped at, so its wait asks
    /// this.
    pub(crate) fn due(&self) -> bool {
        lock(&self.armed).due().is_some()
    }

    /// Count `cost` toward the budget: a completion's, or a decision the advice asked for.
    pub(crate) fn charge(&self, cost: f64) {
        lock(&self.armed).spent += cost;
    }

    /// Why the last task was stopped, if it was: a stop that came after its last turn (Ctrl-C
    /// during the final tool call) still counts.
    pub(crate) fn halted(&self) -> Option<Halt> {
        let halt = lock(&self.armed).halt;
        halt.or_else(|| (self.stopped)().then_some(Halt::Interrupted))
    }
}

impl Chat for Brake<'_> {
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        // The callbacks run with no lock held (R6).
        let stop = match ((self.stopped)(), (self.closed)()) {
            (true, _) => Some(Halt::Interrupted),
            (false, true) => Some(Halt::Closed),
            (false, false) => None,
        };
        let (halt, until) = {
            let mut armed = lock(&self.armed);
            let halt = stop.or_else(|| armed.due());
            armed.halt = halt.or(armed.halt);
            (halt, armed.deadline)
        };
        match halt {
            Some(halt) => Err(ChatError::Unavailable(match halt {
                Halt::TimedOut => "the time limit ran out".into(),
                Halt::OverBudget => "the budget ran out".into(),
                Halt::Closed => "the output was closed".into(),
                Halt::Interrupted => STOPPED.into(),
            })),
            None => {
                // The request is not waited for past the task's time (C2: a value, not a timer).
                let deadline = [request.deadline, until].into_iter().flatten().min();
                let done = self.chat.complete(Request {
                    deadline,
                    ..request
                })?;
                self.charge(done.usage.cost);
                Ok(done)
            }
        }
    }
}

#[cfg(test)]
#[path = "brake_tests.rs"]
mod tests;
