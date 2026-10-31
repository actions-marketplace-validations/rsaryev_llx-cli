//! What the session spent in all: the saved session's steps to begin with, then each model turn
//! (its agents' too). The interface is told the whole of it
//! each time it grows.

use contracts::history::{Session, Spent};

use super::Output;
use crate::sync::lock;

impl Output {
    /// Begin from what `session`'s tasks spent before: a resumed session shows its whole spend.
    pub(crate) fn spent_before(&self, session: &Session) {
        let steps = session.turns.iter().flat_map(|turn| &turn.steps);
        let before = steps.fold(Spent::default(), |all, step| add(all, step.spent));
        *lock(&self.total) = before;
        if !before.is_empty() {
            self.tell_total(before);
        }
    }

    /// What the session spent in all so far.
    pub(crate) fn total(&self) -> Spent {
        *lock(&self.total)
    }

    /// `spent` more, told to the interface.
    pub(crate) fn spend(&self, spent: Spent) {
        let total = {
            let mut total = lock(&self.total);
            *total = add(*total, spent);
            *total
        };
        self.tell_total(total);
    }

    fn tell_total(&self, total: Spent) {
        if let Some(ui) = &self.ui {
            ui.send(protocol::Event::Total(protocol::Total {
                input_tokens: total.input_tokens,
                cached_tokens: total.cached_tokens,
                output_tokens: total.output_tokens,
                cost: total.cost,
            }));
        }
    }
}

fn add(all: Spent, more: Spent) -> Spent {
    Spent {
        input_tokens: all.input_tokens + more.input_tokens,
        cached_tokens: all.cached_tokens + more.cached_tokens,
        output_tokens: all.output_tokens + more.output_tokens,
        cost: all.cost + more.cost,
    }
}

#[cfg(test)]
#[path = "spent_tests.rs"]
mod tests;
