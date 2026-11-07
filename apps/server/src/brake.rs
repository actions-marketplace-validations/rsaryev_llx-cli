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
    /// `None` is no bound.
    pub time: Option<Duration>,
    /// Dollars; `None` is no bound.
    pub budget: Option<f64>,
}

/// Whether a person watches the task: one who does stops it when they will.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Attended {
    /// A session: the person stops a task, which is never cut off at a time of its own.
    Yes,
    /// `llx exec`: nobody is there, and a task that hangs must end.
    No,
}

/// How long an unattended task may take when no time is given.
const UNATTENDED: Duration = Duration::from_secs(1800);

impl Limits {
    /// The limits of a task: `seconds` and `budget` as the command line gives them, else no time
    /// limit for a task a person watches and half an hour for one nobody does.
    pub(crate) fn of(seconds: Option<u64>, budget: Option<f64>, attended: Attended) -> Self {
        let time = match (seconds, attended) {
            (Some(seconds), _) => Some(Duration::from_secs(seconds)),
            (None, Attended::Yes) => None,
            (None, Attended::No) => Some(UNATTENDED),
        };
        Self { time, budget }
    }

    /// Bounded in time alone.
    #[cfg(test)]
    pub(crate) fn time(time: Duration) -> Self {
        Self {
            time: Some(time),
            budget: None,
        }
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

/// Why the brake let no request out, as the model's error.
fn halted(halt: Halt) -> ChatError {
    ChatError::Unavailable(match halt {
        Halt::TimedOut => "the time limit ran out".into(),
        Halt::OverBudget => "the budget ran out".into(),
        Halt::Closed => "the output was closed".into(),
        Halt::Interrupted => STOPPED.into(),
    })
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
            deadline: (limits.time).and_then(|time| Instant::now().checked_add(time)),
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
        if let Some(halt) = halt {
            return Err(halted(halt));
        }
        // The request is not waited for past the task's time (C2: a value, not a timer).
        let deadline = [request.deadline, until].into_iter().flatten().min();
        match self.chat.complete(Request {
            deadline,
            ..request
        }) {
            Ok(done) => {
                self.charge(done.usage.cost);
                Ok(done)
            }
            // A request the time limit cut off: the task ran out of time, whatever the host's
            // words for the cut were, and ends so (with the limit named), not as a failure.
            Err(error) => {
                let due = lock(&self.armed).due();
                match due {
                    Some(Halt::TimedOut) => Err(halted(Halt::TimedOut)),
                    Some(Halt::OverBudget | Halt::Closed | Halt::Interrupted) | None => Err(error),
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "brake_tests.rs"]
mod tests;
