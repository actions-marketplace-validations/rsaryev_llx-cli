//! What `--schema` makes of `result` (offered with every request, so the tools never change and
//! the prompt cache holds): in the work's turns, a call alone, its data matching and nothing still
//! to come, is the answer; alone and not matching, it goes back to be fixed, twice at most; beside
//! other calls, it is not taken (the work is not done while calls still run) and they run. After
//! an answer in words, the data is asked for (`conclude`): the call forced where the host takes
//! that, any other call answered unrun, words asked again.

use std::borrow::Cow;

use contracts::chat::{Calling, Completion, Ending, Message, Request, TaskId, ToolCall};
use contracts::shape::Shape;
use contracts::tool::{ToolError, ToolOutput, ToolSpec};

use super::{Agent, DATA_NOW, MAX_FIXES, RETURNED, Run};
use crate::calls;
use crate::schema;

/// Told of a `result` call made beside others.
const NOT_ALONE: &str = "The result is not taken: call `result` alone, once the work is done.";
/// Told of another call made when the data is asked for.
const ONLY_RESULT: &str = "Not run: only `result` is asked for now.";
/// Told of a `result` made while an agent's answer or a job's end is still to come.
const NOT_YET: &str = "The result is not taken yet: what is still to come arrived below; call \
    `result` again with it.";

/// What became of the `result` calls of a turn.
#[derive(Debug, PartialEq)]
pub(super) enum Returned<'c> {
    /// A `result` alone, its data matching: the run has it, and the turn is the answer; whether
    /// its words were shown as it came.
    Data { said: bool },
    /// A `result` that kept breaking its schema: the run has why, and the turn is the answer.
    Failed,
    /// A `result` alone that did not match, or came before what was still to come: it went
    /// back, and the run goes on.
    Again,
    /// The calls left to run (all of them, borrowed, when none was `result`), and how the reply
    /// ended for them: whole when the call the cut fell in was the `result` left out.
    Rest(Cow<'c, [ToolCall]>, Ending),
}

impl Agent<'_> {
    /// The `result` calls of `completion` answered, and what is left of the turn.
    pub(super) fn returned<'c>(
        &self,
        completion: &'c Completion,
        fixes: &mut usize,
        run: &mut Run,
    ) -> Returned<'c> {
        let calls = &completion.calls;
        let Some(schema) = self.schema.filter(|_| calls.iter().any(is_result)) else {
            return Returned::Rest(Cow::Borrowed(calls), completion.ending);
        };
        if let ([call], Ending::Whole) = (calls.as_slice(), completion.ending) {
            self.say(completion);
            let said = completion
                .text
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty());
            // What reached the run, or is still to come, comes before the data, as before an
            // answer in words.
            let mut came = self.take();
            if came.is_empty() && self.expecting() {
                came = self.inbox.map(|inbox| inbox.wait()).unwrap_or_default();
            }
            if !came.is_empty() {
                self.refuse(call, NOT_YET, run);
                Self::deliver(run, came);
                return Returned::Again;
            }
            let Err(why) = self.answer_result(call, schema, run) else {
                return Returned::Data { said };
            };
            *fixes += 1;
            if *fixes <= MAX_FIXES {
                return Returned::Again;
            }
            run.data = Some(Err(format!("the result did not match the schema: {why}")));
            return Returned::Failed;
        }
        // Not alone, or cut off at the output limit: answered unrun, and the rest goes on.
        let cut = (completion.ending == Ending::Cut)
            .then(|| calls.last())
            .flatten();
        for call in calls.iter().filter(|call| is_result(call)) {
            let why = match cut {
                Some(last) if std::ptr::eq(last, call) => calls::CUT,
                Some(_) | None => NOT_ALONE,
            };
            self.refuse(call, why, run);
        }
        let ending = match cut.is_some_and(is_result) {
            true => Ending::Whole,
            false => completion.ending,
        };
        let rest = calls
            .iter()
            .filter(|call| !is_result(call))
            .cloned()
            .collect();
        Returned::Rest(Cow::Owned(rest), ending)
    }

    /// The calls of a reply to the data's request that are not `result`, answered unrun: only
    /// the data is asked for then. Whether there were any.
    pub(super) fn refuse_others(&self, completion: &Completion, run: &mut Run) -> bool {
        let others: Vec<&ToolCall> = completion
            .calls
            .iter()
            .filter(|call| !is_result(call))
            .collect();
        for call in &others {
            self.refuse(call, ONLY_RESULT, run);
        }
        !others.is_empty()
    }

    /// With `--schema`, once the work has ended: the `result` call asked for and forced, its data
    /// checked; a mismatch goes back to be fixed, until the fixes run out and the run has none.
    pub(super) fn conclude(&self, run: &mut Run, task: TaskId, specs: &[ToolSpec]) {
        let Some(schema) = self.schema else {
            return;
        };
        run.conversation.push(Message::User(DATA_NOW.into()));
        let mut problems = String::from("no `result` call came");
        for _ in 0..=MAX_FIXES {
            let request = Request {
                conversation: run.conversation.messages(),
                tools: specs,
                calling: Calling::Only(schema::TOOL),
                deadline: None,
                effort: run.conversation.effort(),
                task,
            };
            let answered = self.chat.complete(request);
            let Some(completion) = self.kept(answered, run) else {
                return;
            };
            // Every tool is offered (the prompt cache holds): another call is answered unrun.
            let others = self.refuse_others(&completion, run);
            let Some(call) = (completion.calls.iter()).find(|call| call.name == schema::TOOL)
            else {
                // Words where the host forces no call: asked again, after them.
                if !others
                    && completion
                        .text
                        .as_deref()
                        .is_some_and(|text| !text.is_empty())
                {
                    run.conversation.push(Message::User(DATA_NOW.into()));
                }
                continue;
            };
            match self.answer_result(call, schema, run) {
                Ok(()) => return,
                Err(why) => problems = why,
            }
        }
        run.data = Some(Err(format!(
            "the result did not match the schema: {problems}"
        )));
    }

    /// The data of the `result` call `call`, checked against `schema` and answered: the run has
    /// it, or the model is told what to fix, and the caller why it did not match.
    fn answer_result(
        &self,
        call: &ToolCall,
        schema: &dyn Shape,
        run: &mut Run,
    ) -> Result<(), String> {
        self.observer.call(call);
        match schema::take(&call.arguments, schema) {
            Ok(data) => {
                let outcome = Ok(ToolOutput::from(RETURNED.to_string()));
                self.observer.result(call, &outcome);
                run.conversation.result(&call.id, RETURNED);
                run.data = Some(Ok(data));
                Ok(())
            }
            Err(why) => {
                let said = format!("The result is not taken: {why}.");
                let outcome = Err(ToolError::Failed(said.clone()));
                self.observer.result(call, &outcome);
                run.conversation.failure(&call.id, &said);
                Err(why)
            }
        }
    }

    /// `call` answered as failed, with `why`.
    fn refuse(&self, call: &ToolCall, why: &str, run: &mut Run) {
        let outcome = Err(ToolError::Failed(why.into()));
        self.observer.call(call);
        self.observer.result(call, &outcome);
        run.conversation.failure(&call.id, why);
    }
}

fn is_result(call: &ToolCall) -> bool {
    call.name == schema::TOOL
}
