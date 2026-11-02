//! A turn's calls carried out and answered in the conversation.

use contracts::chat::{Completion, Ending, Message, ToolCall};
use contracts::tool::{Tool, ToolError, ToolSpec};

use super::{Agent, Run, Status};
use crate::calls::{self, Outcome};

/// The answer to a call left unrun because one before it in its turn failed.
const NOT_RUN: &str = "not run: a call before it in this turn failed, and this one was written \
    expecting that call to succeed";

/// Replies cut off in a row, with nothing done, that a task goes on from; the next one ends it
/// with an error. Claude Code allows as many.
const MAX_CUTS: usize = 3;
/// What the model is told when its output limit cut a reply off before it ended.
const GO_ON: &str = "Your last reply was cut off at your output limit before it ended. Go on \
from where it stopped, without repeating it; do a long piece of work in smaller steps.";
/// The same, when it was cut off while it was still thinking, its thought kept before this.
const GO_ON_THINKING: &str = "Your last reply was cut off at your output limit while you were \
still thinking; what you had thought is above. Go on from it without thinking it through again, \
and act: do a long piece of work in smaller steps.";

/// One more reply cut off with nothing done; whether that is one too many in a row, the run
/// ended with the error.
pub(super) fn cut_off(cuts: &mut usize, run: &mut Run) -> bool {
    *cuts += 1;
    if *cuts <= MAX_CUTS {
        return false;
    }
    run.status = Status::Error(format!(
        "the model's replies were cut off at its output limit {cuts} times in a row"
    ));
    true
}

/// A reply cut off with no call goes on: what it said is already kept, and when it said nothing,
/// what it thought is kept as its words. A host drops a reasoning that a user message follows, so
/// the model would think it all through again, to be cut off again. Words alone: the reasoning's
/// own parts, cut midway, may be refused when they go back.
pub(super) fn go_on(completion: &Completion, run: &mut Run) {
    let said = (completion.text.as_deref()).is_some_and(|text| !text.trim().is_empty());
    let thought = (completion.thought.as_deref())
        .map(str::trim)
        .filter(|thought| !said && !thought.is_empty());
    let note = match thought {
        Some(thought) => {
            run.conversation.push(Message::Assistant {
                text: Some(thought.into()),
                calls: vec![],
                carried: vec![],
            });
            GO_ON_THINKING
        }
        None => GO_ON,
    };
    run.conversation.push(Message::User(note.into()));
}

/// What a turn's calls came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Came {
    /// A fatal failure ended the run.
    Ended,
    /// At least one call did what it asked.
    Did,
    /// Every call failed or was left unrun.
    Nothing,
}

impl Agent<'_> {
    /// Carry out a turn's calls, those that may run together at once. Once a call fails, the
    /// turn's later calls are answered unrun: they were written expecting it to succeed (a check
    /// after a change that did not apply would test the old files). The last call of a reply the
    /// output limit cut off is never run as it stands: only what its tool can tell is whole (B7).
    pub(super) fn handle_all(
        &self,
        calls: &[ToolCall],
        ending: Ending,
        specs: &[ToolSpec],
        run: &mut Run,
    ) -> Came {
        let (whole, cut) = match (ending, calls.split_last()) {
            (Ending::Cut, Some((last, rest))) => (rest, Some(last)),
            (Ending::Cut | Ending::Whole, _) => (calls, None),
        };
        let parallel = |call: &ToolCall| calls::parallel(self.tools, specs, call);
        let (mut ends, mut failed, mut did) = (false, false, false);
        let mut start = 0;
        while start < whole.len() && !ends && !failed {
            let end = calls::batch_end(whole, start, parallel);
            let batch = &whole[start..end];
            let outcomes = if end - start == 1 {
                vec![self.handle(&whole[start], specs, calls::invoke)]
            } else {
                calls::together(batch, self.tools, specs, self.observer)
            };
            for (call, outcome) in batch.iter().zip(outcomes) {
                failed |= outcome.is_err();
                did |= outcome.is_ok();
                ends |= Self::record(call, outcome, run);
            }
            start = end;
        }
        if let Some(call) = cut.filter(|_| !ends && !failed) {
            let outcome = self.handle(call, specs, calls::salvage);
            did |= outcome.is_ok();
            ends |= Self::record(call, outcome, run);
            start = calls.len();
        }
        if ends {
            return Came::Ended;
        }
        for call in &calls[start..] {
            // A cut call is told so: sent again as it was, it would be cut again.
            let why = match cut {
                Some(cut) if std::ptr::eq(cut, call) => calls::CUT,
                Some(_) | None => NOT_RUN,
            };
            let outcome = Err(ToolError::Failed(why.into()));
            self.observer.call(call);
            self.observer.result(call, &outcome);
            answer_call(run, call, outcome);
        }
        if did { Came::Did } else { Came::Nothing }
    }

    /// Carry out `call` with `how`, shown to the observer as it starts and ends.
    fn handle(
        &self,
        call: &ToolCall,
        specs: &[ToolSpec],
        how: fn(&[&dyn Tool], &[ToolSpec], &ToolCall) -> Outcome,
    ) -> Outcome {
        self.observer.call(call);
        let outcome = how(self.tools, specs, call);
        self.observer.result(call, &outcome);
        outcome
    }

    /// Answer `call` with `outcome`; whether the run ends with it (a fatal failure).
    fn record(call: &ToolCall, outcome: Outcome, run: &mut Run) -> bool {
        let ends = match &outcome {
            Ok(_) | Err(ToolError::Failed(_)) => false,
            Err(ToolError::Fatal(why)) => {
                run.status = Status::Error(why.clone());
                true
            }
        };
        answer_call(run, call, outcome);
        ends
    }
}

/// Answer `call` in the conversation.
fn answer_call(run: &mut Run, call: &ToolCall, outcome: Outcome) {
    match outcome {
        Ok(output) => run.conversation.result(&call.id, &output.report),
        Err(error) => run.conversation.failure(&call.id, &error.to_string()),
    }
}
