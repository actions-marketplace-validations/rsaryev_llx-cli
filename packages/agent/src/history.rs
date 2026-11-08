//! How a conversation is prepared for the model before each request: a [`Strategy`], chosen by
//! the app. Each participant (the main agent, each agent it started) keeps its own conversation
//! and is fitted to its own model, so one strategy serves them all.
//!
//! [`Pipeline`] is the strategy llx uses: named `Stage`s, least valuable first, each eliding
//! what it owns (old reasoning, outputs, arguments), outputs and arguments for pointers that say
//! how to get them back. No summary. Made for the prompt cache:
//! - nothing runs while the conversation is under `HIGH` of the model's limit (a pruned prefix
//!   trades cheap cached tokens for a rewrite of everything after it);
//! - once it is, the stages run until `LOW`, so one cache miss buys many stable turns;
//! - a pass that could not get down to `LOW` (what no stage elides, a model's reasoning carried
//!   back, its words, came to more) opens again only once the conversation has grown by
//!   `HIGH - LOW` since: else every request would elide the one output just past the tail, and
//!   miss the cache from there;
//! - an elision is made in the conversation itself, so it is never undone (a ratchet);
//! - the recent tail, `tail` tokens and at least `MIN_TAIL` messages, is left alone unless the
//!   model's own limit is passed (not llx's ceiling, `AUTO`, under it: past that, a conversation
//!   the stages cannot bring down goes on whole rather than lose what it just read);
//! - so is the newest output of each call the app keeps ([`Lasting`]): what is still in force once
//!   read, as the rules of a directory are.

use std::collections::{HashMap, HashSet};

use contracts::chat::{Message, ToolCall};
use contracts::tool::ToolSpec;
use serde_json::Value;

use crate::conversation::Conversation;

/// The window assumed when the model's is not known.
const FALLBACK: u64 = 128_000;
/// The most a request holds, whatever the window: quality and cost fall well before a million.
const AUTO: u64 = 272_000;
/// Messages at the end the tail always holds, however many tokens they take; past the ceiling,
/// all but these give way.
const MIN_TAIL: usize = 4;
/// Tokens of the recent tail llx leaves alone.
const TAIL: u64 = 16_384;
/// Percent of the ceiling that opens the gate.
const HIGH: u64 = 90;
/// Percent of the ceiling a pass stops at.
const LOW: u64 = 60;
/// Bytes below which nothing is worth eliding.
const MIN_BYTES: usize = 400;
/// What every pointer says, so an elision is told apart.
pub(crate) const MARKER: &str = "elided to save context";
/// What a picture counts for, in tokens.
const IMAGE: u64 = 1_500;

/// The arithmetic of one request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Frame {
    /// Where a pass stops.
    pub(crate) target: u64,
    /// Real tokens per estimated one, from the last answer's usage.
    pub(crate) ratio: f64,
    /// Where the protected tail starts.
    pub(crate) tail: usize,
    pub(crate) pass: Pass,
}

/// Which pass a frame is of.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Pass {
    /// Down to the target: the tail and what the app keeps are left alone.
    Gate,
    /// Past the limit itself: all but the last messages give way.
    Ceiling,
}

/// Whether the app keeps what a call got back through a pass (C3): the agent knows no tool, the
/// app knows which outputs are still in force once read. Given the call and its output.
pub type Lasting = dyn Fn(&ToolCall, &str) -> bool + Send + Sync;

/// What of one value a turn carried back may go once the turn is old: its model's reasoning,
/// which the provider that sealed it knows (C3: the agent knows no provider's format). Takes it
/// out of the value, or leaves the value null when nothing else was in it; whether it changed.
/// A value it does not know it leaves as it is.
pub type Shed = dyn Fn(&mut Value) -> bool + Send + Sync;

impl Frame {
    /// `estimate` tokens, estimated, in real tokens, as far as the ratio tells.
    fn real(&self, estimate: u64) -> u64 {
        whole(estimate as f64 * self.ratio)
    }

    /// The conversation's size in real tokens.
    fn size(&self, conversation: &Conversation) -> u64 {
        self.real(conversation.tokens())
    }
}

/// What a preparation did: each stage that changed something, and how much.
pub type Report = Vec<(&'static str, usize)>;

/// What a request may hold, in real tokens: `ceiling`, what llx keeps it under (the gate opens at
/// `HIGH` of it, a pass stops at `LOW`), and `limit`, what the model itself takes, less a reserve
/// for the answer: past it, the tail gives way too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub ceiling: u64,
    pub limit: u64,
}

impl Limits {
    /// Both less `tokens`: what goes with every request and no stage can elide (the specs).
    #[must_use]
    pub fn less(self, tokens: u64) -> Self {
        Self {
            ceiling: self.ceiling.saturating_sub(tokens),
            limit: self.limit.saturating_sub(tokens),
        }
    }
}

pub trait Strategy: Send + Sync {
    /// The most a request to a model with a `window` of tokens may hold.
    fn ceiling(&self, window: u64) -> u64 {
        self.limits(window).ceiling
    }

    /// What a request to a model with a `window` of tokens may hold: llx's ceiling, and the
    /// model's own limit.
    fn limits(&self, window: u64) -> Limits {
        let window = if window == 0 { FALLBACK } else { window };
        // A reserve for the answer and the growth within a run.
        let reserve = (window / 5).min(16_384).max(window / 10);
        let limit = window - reserve;
        Limits {
            ceiling: limit.min(AUTO),
            limit,
        }
    }

    /// Prepare `conversation` for a request within `limits`; `ratio`: real tokens per estimated
    /// one, from the last answer's usage.
    fn prepare(&self, conversation: &mut Conversation, limits: Limits, ratio: f64) -> Report;
}

/// One step of a pipeline: elides what it owns before the frame's tail, oldest first, until the
/// conversation is down to the frame's target; how much it elided.
pub(crate) trait Stage: Send + Sync {
    fn name(&self) -> &'static str;
    fn run(&self, conversation: &mut Conversation, frame: &Frame) -> usize;
}

/// Stages behind a gate, in order.
pub struct Pipeline {
    stages: Vec<Box<dyn Stage>>,
    /// Tokens of the recent tail left alone.
    tail: u64,
}

impl Default for Pipeline {
    /// llx's: old outputs, old arguments; the tail only past the model's limit.
    fn default() -> Self {
        Self::keeping(Box::new(|_, _| false))
    }
}

impl Pipeline {
    /// llx's, the newest output of each call `keep` names left alone as the tail is; no
    /// reasoning let go.
    pub fn keeping(keep: Box<Lasting>) -> Self {
        Self::shedding(keep, Box::new(|_| false))
    }

    /// llx's: first the reasoning old turns carried back, as `shed` lets it go (the least
    /// valuable: a provider drops it itself once the person speaks again, and in a long task it
    /// grows with every turn), then old outputs but the newest of each call `keep` names, then
    /// old arguments.
    pub fn shedding(keep: Box<Lasting>, shed: Box<Shed>) -> Self {
        Self {
            stages: vec![
                Box::new(Reasoning { shed }),
                Box::new(Outputs { keep }),
                Box::new(Arguments),
            ],
            tail: TAIL,
        }
    }
}

impl Strategy for Pipeline {
    fn prepare(&self, conversation: &mut Conversation, limits: Limits, ratio: f64) -> Report {
        let Limits { ceiling, limit } = limits;
        let mut frame = Frame {
            target: ceiling * LOW / 100,
            ratio,
            tail: 0,
            pass: Pass::Gate,
        };
        let mut report = Report::new();
        let size = frame.size(conversation);
        // At the gate, and a pass's headroom past where the last one left it.
        let due = size * 100 >= ceiling * HIGH
            && size >= conversation.settled() + ceiling * (HIGH - LOW) / 100;
        if !due && size <= limit {
            return report;
        }
        // The tail is counted in real tokens too.
        frame.tail = tail(conversation, whole(self.tail as f64 / ratio.max(0.01)));
        let mut run = |frame: &Frame, conversation: &mut Conversation| {
            for stage in &self.stages {
                if frame.size(conversation) <= frame.target {
                    break;
                }
                let elided = stage.run(conversation, frame);
                if elided > 0 {
                    report.push((stage.name(), elided));
                }
            }
        };
        run(&frame, conversation);
        // Still past the model's own limit: the tail gives way too, all but its last messages.
        if frame.size(conversation) > limit {
            frame.tail = conversation.messages().len().saturating_sub(MIN_TAIL);
            frame.pass = Pass::Ceiling;
            run(&frame, conversation);
        }
        conversation.settle(frame.size(conversation));
        report
    }
}

/// The reasoning old turns carried back, as far as the provider's `shed` lets it go.
pub(crate) struct Reasoning {
    shed: Box<Shed>,
}

impl Stage for Reasoning {
    fn name(&self) -> &'static str {
        "reasoning"
    }

    fn run(&self, conversation: &mut Conversation, frame: &Frame) -> usize {
        // The model's last turn keeps its reasoning, however far its results pushed it from the
        // end: a provider may take its next request only with it (thinking in a tool loop).
        let last = (conversation.messages().iter())
            .rposition(|message| matches!(message, Message::Assistant { .. }));
        pass(conversation, frame, |conversation, at| {
            match Some(at) == last {
                true => 0,
                false => conversation.shed(at, &*self.shed),
            }
        })
    }
}

/// Tool outputs, any worth eliding but those the app keeps.
pub(crate) struct Outputs {
    keep: Box<Lasting>,
}

impl Stage for Outputs {
    fn name(&self) -> &'static str {
        "outputs"
    }

    fn run(&self, conversation: &mut Conversation, frame: &Frame) -> usize {
        let kept = match frame.pass {
            Pass::Gate => kept(conversation.messages(), &self.keep),
            Pass::Ceiling => HashSet::new(),
        };
        elide(conversation, frame, |message| {
            matches!(message, Message::ToolResult { id, content, .. }
                if worth(content) && !kept.contains(id))
        })
    }
}

/// The ids of the outputs `keep` names, the newest of each call (its tool and arguments): a file
/// read again keeps one copy, not every one.
fn kept(messages: &[Message], keep: &Lasting) -> HashSet<String> {
    let mut calls: HashMap<&str, &ToolCall> = HashMap::new();
    let mut newest = HashMap::new();
    for message in messages {
        match message {
            Message::Assistant { calls: made, .. } => {
                calls.extend(made.iter().map(|call| (call.id.as_str(), call)));
            }
            Message::ToolResult {
                id,
                content,
                failed: false,
            } => {
                if let Some(call) = calls.get(id.as_str()).filter(|call| keep(call, content)) {
                    newest.insert((&call.name, call.arguments.to_string()), id.clone());
                }
            }
            Message::ToolResult { failed: true, .. }
            | Message::System(_)
            | Message::User(_)
            | Message::Image { .. } => {}
        }
    }
    newest.into_values().collect()
}

/// The large arguments of tool calls: what an edit wrote is on disk.
pub(crate) struct Arguments;

impl Stage for Arguments {
    fn name(&self) -> &'static str {
        "arguments"
    }

    fn run(&self, conversation: &mut Conversation, frame: &Frame) -> usize {
        elide(conversation, frame, |message| {
            matches!(message, Message::Assistant { calls, .. }
                if calls.iter().any(|call| large(&call.arguments)))
        })
    }
}

/// Elide the messages before the frame's tail that a stage `owns`, oldest first, until the
/// frame's target.
fn elide(conversation: &mut Conversation, frame: &Frame, owns: impl Fn(&Message) -> bool) -> usize {
    pass(conversation, frame, |conversation, at| {
        match conversation.messages().get(at).is_some_and(&owns) {
            true => conversation.elide(at),
            false => 0,
        }
    })
}

/// `act` on each message before the frame's tail, oldest first, until the frame's target: how
/// much it did. The size is counted once and kept by what each act took out, not read again from
/// the whole conversation.
fn pass(
    conversation: &mut Conversation,
    frame: &Frame,
    mut act: impl FnMut(&mut Conversation, usize) -> usize,
) -> usize {
    let mut estimate = conversation.tokens();
    let mut done = 0;
    for at in 0..frame.tail {
        if frame.real(estimate) <= frame.target {
            break;
        }
        let before = conversation.size_at(at);
        done += act(conversation, at);
        estimate = estimate - before + conversation.size_at(at);
    }
    done
}

/// Where the tail of `budget` tokens starts: at least `MIN_TAIL` messages from the end. The
/// sizes are those kept with the messages: worked out again, every call's arguments in the tail
/// would be written out once more.
fn tail(conversation: &Conversation, budget: u64) -> usize {
    let len = conversation.messages().len();
    let (mut size, mut start) = (0, len);
    while start > 0 {
        size += conversation.size_at(start - 1);
        if size > budget && len - start >= MIN_TAIL {
            break;
        }
        start -= 1;
    }
    start
}

/// Whether a text is worth eliding. A pointer is far shorter than this, so it never is: an
/// output that only quotes the marker is elided as any other.
pub(crate) fn worth(text: &str) -> bool {
    text.len() >= MIN_BYTES
}

/// Whether a call's arguments hold a text worth eliding: a value of its object, or the text
/// itself for a text tool.
fn large(arguments: &Value) -> bool {
    arguments.as_object().map_or_else(
        || arguments.as_str().is_some_and(worth),
        |map| map.values().any(|v| v.as_str().is_some_and(worth)),
    )
}

/// An estimate in whole tokens.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`as` saturates and drops the fraction: whole tokens are meant, and none below zero"
)]
pub(crate) fn whole(tokens: f64) -> u64 {
    tokens as u64
}

/// A message's tokens, estimated: four bytes of its text to a token, what its turn carried back
/// with it (a model's reasoning, sent with every request of the task, and counted by the model).
/// Only the model's own is ever there (`Conversation::for_model`): another's is never sent.
pub(crate) fn tokens(message: &Message) -> u64 {
    let bytes = match message {
        Message::System(text) | Message::User(text) => text.len(),
        Message::Assistant {
            text,
            calls,
            carried,
        } => {
            let arguments: usize = calls.iter().map(|c| c.arguments.to_string().len()).sum();
            let kept: usize = carried.iter().map(|kept| kept.to_string().len()).sum();
            text.as_deref().map_or(0, str::len) + arguments + kept
        }
        Message::ToolResult { content, .. } => content.len(),
        Message::Image { .. } => return IMAGE,
    };
    bytes as u64 / 4 + 1
}

/// The tool specs' tokens, estimated as a message's are.
pub(crate) fn specs(specs: &[ToolSpec]) -> u64 {
    let size = |spec: &ToolSpec| {
        spec.name.len() + spec.description.len() + spec.input.schema().to_string().len()
    };
    specs.iter().map(|spec| size(spec) as u64 / 4).sum()
}

/// The pointer left in place of an output of `chars` characters.
pub(crate) fn pointer(chars: usize) -> String {
    format!(
        "[output {MARKER}: {chars} characters. Call the tool again if you still need it; if the \
         file changed since, read it again rather than relying on what was here]"
    )
}

/// The pointer left in place of an argument of `chars` characters.
pub(crate) fn argument(chars: usize) -> String {
    format!("[{MARKER}: {chars} characters]")
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
