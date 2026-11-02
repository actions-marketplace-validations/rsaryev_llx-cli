//! The agent's loop: the chat model reads the goal and calls tools until it answers with a
//! message and no call, bounded by `max_turns` (unlimited unless the app sets one). That message
//! is the answer; with `--schema`, the data is then asked for in one more request with the
//! `result` call forced (`crate::schema`). Nothing invites the model to stop: a failed call goes
//! back to it with its reason, and no tool ends the task. Only the advisor pushes an answer on,
//! twice a task at most (C9, `agent/advice.rs`).
//! The calls of one turn that may run together do (`crate::calls`). A reply the output limit cut
//! off is not an answer: the model goes on from it, and its last call is never run as it stands
//! (B7, `agent/handling.rs`). Before each turn, what came into the inbox is said; an answer while
//! something is still to come waits for it, and the run goes on.

mod advice;
mod handling;

use std::sync::atomic::{AtomicU64, Ordering};

use advice::Begun;
use handling::{Came, cut_off, go_on};

use contracts::advice::Advisor;
use contracts::chat::{
    Calling, Chat, ChatError, Completion, Ending, Message, Request, Spend, TaskId,
};
use contracts::inbox::Inbox;
use contracts::tool::{Image, Tool, ToolError, ToolOutput, ToolSpec};
use serde_json::Value;

use crate::conversation::Conversation;
use crate::history::{self, Strategy, whole};
use crate::observer::Observer;
use crate::schema;

const SYSTEM: &str = include_str!("prompt.md");
/// Times a `result` that breaks its schema goes back to be fixed.
const MAX_FIXES: usize = 2;
/// Asked once the work has ended, with `--schema`: the request that forces the `result` call.
const DATA_NOW: &str = "Return the data the schema asks for, as `result`.";
/// The result a matching `result` call gets, so a follow-up finds no call left open.
const RETURNED: &str = "The result was returned.";
/// The tasks begun in this process, so no two are ever one (a rewind's included), whichever
/// agent began them: the first is 1, as 0 is no task's.
static TASKS: AtomicU64 = AtomicU64::new(1);

/// One participant's loop (the main session, or an agent it started): its model, its tools and
/// how its conversation is kept; [`Agent::run`] reaches each goal.
pub struct Agent<'a> {
    pub chat: &'a dyn Chat,
    pub tools: &'a [&'a dyn Tool],
    /// What the app knows of where the run happens (the working directory, the system, the date,
    /// the project's instructions): it opens a new conversation, before the first goal. Empty for
    /// none.
    pub context: &'a str,
    /// Model turns for one goal.
    pub max_turns: usize,
    /// Sees each call, its result and each turn as they happen (the live trace).
    pub observer: &'a dyn Observer,
    /// `--schema`: the `result` call carries data that matches it.
    pub schema: Option<&'a Value>,
    /// What reaches the run between its turns; `None` for nothing.
    pub inbox: Option<&'a dyn Inbox>,
    /// The model's context window in tokens (0 when it is not known): the strategy fits the
    /// conversation to the limit it leaves.
    pub window: u64,
    /// How the conversation is prepared for the model before each request.
    pub strategy: &'a dyn Strategy,
    /// A decision model's word at the task's edges (C9); `None`: the loop as it is without one.
    pub advisor: Option<&'a dyn Advisor>,
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// The model answered: a message with no call.
    Done,
    /// Turns ran out before an answer.
    Exhausted,
    /// The run could not go on, and why: the model, or what a tool depends on, could not be
    /// reached, or the model's replies kept being cut off at its output limit.
    Error(String),
}

/// What a run came to.
pub struct Run {
    pub status: Status,
    pub answer: String,
    /// The answer was already shown as the model's words (`Observer::said`): said as a wait for
    /// what was still to come began, and nothing came. The app does not show it again.
    pub shown: bool,
    /// `--schema`: the data, or why none matched.
    pub data: Option<Result<Value, String>>,
    pub spend: Spend,
    /// The conversation as it ended: a follow-up goal goes on from it.
    pub conversation: Conversation,
    /// The tokens the conversation took at its last turn.
    pub used: u64,
}

/// What became of an answer held for what was still to come.
enum Waited {
    /// Something came: the run goes on with it.
    Came,
    /// The answer was said as the wait began, and nothing came: it stands, already shown.
    Said,
    /// Nothing was waited for: the answer stands, not yet shown.
    No,
}

impl Agent<'_> {
    /// Reach `goal` in `conversation`, after what was done there before, or in a new one, which
    /// opens with the context (an earlier one already does: it is not repeated); `attached`, the
    /// pictures the person attached to the goal, each with its name.
    pub fn run(
        &self,
        conversation: Option<Conversation>,
        goal: &str,
        attached: &[(String, Image)],
    ) -> Run {
        let mut conversation = self.tell(conversation, &format!("Goal: {goal}"));
        let begun = Begun::at(goal, &conversation);
        for (name, image) in attached {
            conversation.attach(name, image.clone());
        }
        self.reach(conversation, begun)
    }

    /// Say `message` at the end of `conversation` (its open calls closed first), or open a new one
    /// with it after the context: a goal, or something the app says between goals (a setting).
    ///
    /// The context is a message of its own, before the first goal: it is the same for every task
    /// and session in a workspace, so the prompt cache keeps it, while each goal comes after it.
    pub fn tell(&self, conversation: Option<Conversation>, message: &str) -> Conversation {
        let mut conversation = conversation.unwrap_or_else(|| {
            let mut opened = Conversation::new(SYSTEM);
            let context = self.context.trim();
            if !context.is_empty() {
                opened.push(Message::User(format!("<context>\n{context}\n</context>")));
            }
            opened
        });
        conversation.close_open_calls();
        conversation.push(Message::User(message.to_string()));
        conversation
    }

    fn reach(&self, conversation: Conversation, begun: Begun<'_>) -> Run {
        let task = TaskId(TASKS.fetch_add(1, Ordering::Relaxed));
        let specs: Vec<ToolSpec> = self.tools.iter().map(|tool| tool.spec()).collect();
        let mut run = Run {
            status: Status::Exhausted,
            answer: String::new(),
            shown: false,
            data: None,
            spend: Spend::default(),
            conversation,
            used: 0,
        };
        // Real tokens per estimated one, from the last answer's usage.
        let mut ratio = 1.0;
        // The specs go with every request: they count in the usage the ratio is taken from, and
        // against the limit, but no stage can elide them.
        let fixed = history::specs(&specs);
        self.advise_start(&begun, &mut run);
        let (mut cuts, mut pushes) = (0, 0);
        for turn in 0..self.max_turns {
            Self::deliver(&mut run, self.take());
            // Prepared for the model before each request; the elided stays elided.
            let ceiling = self
                .strategy
                .ceiling(self.window)
                .saturating_sub(whole(fixed as f64 * ratio));
            let report = self.strategy.prepare(&mut run.conversation, ceiling, ratio);
            if !report.is_empty() {
                self.observer.fitted(&report);
            }
            let estimate = run.conversation.tokens();
            let request = Request {
                conversation: run.conversation.messages(),
                tools: &specs,
                calling: Calling::Any,
                deadline: None,
                effort: run.conversation.effort(),
                task,
            };
            let answered = self.chat.complete(request);
            let Some(completion) = self.kept(answered, &mut run) else {
                return run;
            };
            if completion.usage.input_tokens > 0 && estimate > 0 {
                let input = completion.usage.input_tokens as f64;
                ratio = (input / (estimate + fixed) as f64).clamp(0.3, 4.0);
            }
            // A reply cut off with no call is not the answer: what it wrote stays, and the model
            // goes on from it, as Claude Code's does, where Codex and opencode end the task.
            if completion.ending == Ending::Cut && completion.calls.is_empty() {
                if cut_off(&mut cuts, &mut run) {
                    return run;
                }
                self.say(&completion);
                go_on(&completion, &mut run);
                continue;
            }
            if completion.ending == Ending::Whole {
                cuts = 0;
            }
            // An answer waits for what is still to come (not on the last turn: then it stands).
            let mut said = false;
            if turn + 1 < self.max_turns {
                match self.awaited(&completion, &mut run) {
                    Waited::Came => continue,
                    Waited::Said => said = true,
                    Waited::No => {}
                }
            }
            if completion.calls.is_empty() {
                // A message with no call is the answer: the task ends with it, as a person's
                // final message ends their turn, unless the advisor finds it unfinished.
                let answer = completion.text.unwrap_or_default().trim().to_string();
                if turn + 1 < self.max_turns
                    && self.pushed_on(&begun, &answer, &mut pushes, &mut run)
                {
                    continue;
                }
                run.status = Status::Done;
                run.answer = answer;
                run.shown = said;
                self.conclude(&mut run, task);
                return run;
            }
            self.say(&completion);
            match self.handle_all(&completion.calls, completion.ending, &specs, &mut run) {
                Came::Ended => return run,
                Came::Did => cuts = 0,
                // A cut reply that did nothing counts as one with no call does.
                Came::Nothing if completion.ending == Ending::Cut => {
                    if cut_off(&mut cuts, &mut run) {
                        return run;
                    }
                }
                Came::Nothing => {}
            }
        }
        run
    }

    /// A request's answer counted and kept in the conversation. `None` when the model could not
    /// be reached: the run has ended with the error.
    fn kept(&self, answered: Result<Completion, ChatError>, run: &mut Run) -> Option<Completion> {
        let mut completion = match answered {
            Ok(completion) => completion,
            Err(error) => {
                run.status = Status::Error(error.to_string());
                return None;
            }
        };
        self.observer.turn(&completion.usage);
        run.spend.add(&completion.usage);
        run.used = completion.usage.input_tokens + completion.usage.output_tokens;
        // A reply that says nothing and calls nothing is not kept: some hosts reject it.
        let said = completion
            .text
            .as_deref()
            .is_some_and(|text| !text.is_empty());
        if said || !completion.calls.is_empty() {
            // What goes back with the turn (a sealed reasoning, often the largest part of it) is
            // the conversation's alone from here: moved, not copied.
            run.conversation.push(Message::Assistant {
                text: completion.text.clone(),
                calls: completion.calls.clone(),
                carried: std::mem::take(&mut completion.carried),
            });
        }
        Some(completion)
    }

    /// With `--schema`, once the work has ended: the `result` call asked for and forced, its data
    /// checked; a mismatch goes back to be fixed, until the fixes run out and the run has none.
    fn conclude(&self, run: &mut Run, task: TaskId) {
        let Some(schema) = self.schema else {
            return;
        };
        let specs = [schema::spec(schema)];
        run.conversation.push(Message::User(DATA_NOW.into()));
        let mut problems = String::from("no `result` call came");
        for _ in 0..=MAX_FIXES {
            let request = Request {
                conversation: run.conversation.messages(),
                tools: &specs,
                calling: Calling::Only(schema::TOOL),
                deadline: None,
                effort: run.conversation.effort(),
                task,
            };
            let answered = self.chat.complete(request);
            let Some(completion) = self.kept(answered, run) else {
                return;
            };
            let Some(call) = (completion.calls.iter()).find(|call| call.name == schema::TOOL)
            else {
                break;
            };
            self.observer.call(call);
            match schema::take(&call.arguments, schema) {
                Ok(data) => {
                    let outcome = Ok(ToolOutput::from(RETURNED.to_string()));
                    self.observer.result(call, &outcome);
                    run.conversation.result(&call.id, RETURNED);
                    run.data = Some(Ok(data));
                    return;
                }
                Err(why) => {
                    let said = format!("The result is not taken: {why}.");
                    let outcome = Err(ToolError::Failed(said.clone()));
                    self.observer.result(call, &outcome);
                    run.conversation.failure(&call.id, &said);
                    problems = why;
                }
            }
        }
        run.data = Some(Err(format!(
            "the result did not match the schema: {problems}"
        )));
    }

    /// Whether an answer (a message with no call) was held for what reached the run meanwhile,
    /// or is still to come: then it was delivered and the run goes on. Whether more is to come is
    /// asked before taking: a source that is done posts its last entry before it says so, so
    /// nothing it sent is left behind. On the last turn the answer stands (the caller skips
    /// this), and what is still to come waits for the next goal.
    fn awaited(&self, completion: &Completion, run: &mut Run) -> Waited {
        if !completion.calls.is_empty() {
            return Waited::No;
        }
        let expecting = self.expecting();
        let came = self.take();
        let waits = came.is_empty() && expecting;
        if waits || !came.is_empty() {
            self.say(completion);
        }
        let came = match waits {
            true => self.inbox.map(|inbox| inbox.wait()).unwrap_or_default(),
            false => came,
        };
        match (came.is_empty(), waits) {
            (false, _) => {
                Self::deliver(run, came);
                Waited::Came
            }
            (true, true) => Waited::Said,
            (true, false) => Waited::No,
        }
    }

    /// Show what the model said in a turn the run goes on from: it is not the answer.
    fn say(&self, completion: &Completion) {
        let text = completion.text.as_deref().map(str::trim);
        if let Some(text) = text.filter(|text| !text.is_empty()) {
            self.observer.said(text);
        }
    }

    fn take(&self) -> Vec<String> {
        self.inbox.map(|inbox| inbox.take()).unwrap_or_default()
    }

    fn expecting(&self) -> bool {
        self.inbox.is_some_and(|inbox| inbox.expecting())
    }

    /// Say what came in, after the results of the last turn's calls.
    fn deliver(run: &mut Run, came: Vec<String>) {
        for message in came {
            run.conversation.push(Message::User(message));
        }
    }
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
