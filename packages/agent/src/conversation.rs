//! The conversation as the agent keeps it: messages that only grow, so every turn reads its whole
//! prefix from the prompt cache. A result too long for one message keeps its head and tail. A
//! picture the person attached stays.

use std::collections::HashSet;

use contracts::catalog::Effort;
use contracts::chat::Message;
use contracts::tool::Image;
use serde_json::Value;

use crate::history as fit;

/// Characters one tool result may take (about 22k tokens). A last resort only: each tool keeps its
/// own output under this (a command's streams), so the model is never told it saw a whole output
/// whose middle was cut here.
const MAX_RESULT: usize = 90_000;
/// What a call left without a result (a run that ended mid-turn) is answered with.
const UNANSWERED: &str = "The call was not carried out: the run ended first.";

/// The messages, each one's estimated size, and the level the conversation thinks at.
#[derive(Debug)]
pub struct Conversation {
    messages: Vec<Message>,
    /// Each message's estimated tokens, kept with it: the estimate is asked for at every turn,
    /// and working it out means writing every call's arguments out again.
    sizes: Vec<u64>,
    /// The level it thinks at, where it is not the model's chosen one: chosen as it opened, kept
    /// as it goes on, so the prompt cache stays whole (C9).
    effort: Option<Effort>,
    /// Real tokens per estimated one, as the model's last usage measured them: kept from task to
    /// task, so a follow-up's first request is not sized as if nothing were known (an estimate
    /// that runs high would open the history's gate and rewrite a prefix the cache holds).
    ratio: f64,
    /// Real tokens the history's last pass left it at: a pass that could not get down to its
    /// target (what no stage elides came to more) opens again only once the conversation has
    /// grown a pass's headroom past it.
    settled: u64,
}

impl Conversation {
    pub(crate) fn new(system: &str) -> Self {
        let system = Message::System(system.to_string());
        Self {
            sizes: vec![fit::tokens(&system)],
            messages: vec![system],
            effort: None,
            ratio: 1.0,
            settled: 0,
        }
    }

    /// Real tokens per estimated one, as last measured.
    pub(crate) fn ratio(&self) -> f64 {
        self.ratio
    }

    pub(crate) fn measured(&mut self, ratio: f64) {
        self.ratio = ratio;
    }

    /// Real tokens the history's last pass left the conversation at.
    pub(crate) fn settled(&self) -> u64 {
        self.settled
    }

    pub(crate) fn settle(&mut self, tokens: u64) {
        self.settled = tokens;
    }

    /// Let go of what earlier turns carried back for a model other than `model` (each value
    /// names its own): no provider sends it to another.
    pub fn for_model(&mut self, model: &str) {
        for at in 0..self.messages.len() {
            if let Some(Message::Assistant { carried, .. }) = self.messages.get_mut(at) {
                let before = carried.len();
                carried.retain(|kept| kept["model"] == model);
                if carried.len() != before {
                    self.resized(at);
                }
            }
        }
    }

    /// Let message `at`'s carried go as far as `shed` says it may (see `history`): 1 when it
    /// changed. A value `shed` left null had nothing else, and goes.
    pub(crate) fn shed(&mut self, at: usize, shed: &fit::Shed) -> usize {
        let Some(Message::Assistant { carried, .. }) = self.messages.get_mut(at) else {
            return 0;
        };
        let mut changed = false;
        for kept in carried.iter_mut() {
            changed |= shed(kept);
        }
        carried.retain(|kept| !kept.is_null());
        if !changed {
            return 0;
        }
        self.resized(at);
        1
    }

    /// A conversation that goes on from `messages` (another's, as its last request sent them):
    /// the calls they leave open are answered with `unanswered`, as results, not failures.
    pub fn of(messages: Vec<Message>, unanswered: &str) -> Self {
        let mut conversation = Self {
            sizes: messages.iter().map(fit::tokens).collect(),
            messages,
            effort: None,
            ratio: 1.0,
            settled: 0,
        };
        for id in conversation.open_calls() {
            conversation.push(Message::ToolResult {
                id,
                content: unanswered.to_string(),
                failed: false,
            });
        }
        conversation
    }

    /// The level it thinks at, where it is not the model's chosen one.
    pub(crate) fn effort(&self) -> Option<Effort> {
        self.effort
    }

    /// Think at `effort` from now on (a fork keeps its parent's); `None`: the model's chosen one.
    pub fn set_effort(&mut self, effort: Option<Effort>) {
        self.effort = effort;
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// The conversation as it was when it had `len` messages: a rewind to before a task.
    pub fn truncate(&mut self, len: usize) {
        self.messages.truncate(len.max(1));
        self.sizes.truncate(self.messages.len());
        // What the last pass left is no measure of a conversation taken back.
        self.settled = 0;
    }

    pub(crate) fn push(&mut self, message: Message) {
        self.sizes.push(fit::tokens(&message));
        self.messages.push(message);
    }

    /// Message `at` changed in place: its size with it.
    fn resized(&mut self, at: usize) {
        if let (Some(size), Some(message)) = (self.sizes.get_mut(at), self.messages.get(at)) {
            *size = fit::tokens(message);
        }
    }

    /// Answer every tool call left without a result, so the conversation can go on: a model
    /// refuses one whose calls are not all answered.
    pub(crate) fn close_open_calls(&mut self) {
        for id in self.open_calls() {
            self.failure(&id, UNANSWERED);
        }
    }

    /// The calls with no result yet: the answered ids gathered in one pass, then each call's
    /// looked up.
    fn open_calls(&self) -> Vec<String> {
        let mut answered = HashSet::new();
        let mut calls = vec![];
        for message in &self.messages {
            match message {
                Message::Assistant { calls: made, .. } => calls.extend(made),
                Message::ToolResult { id, .. } => {
                    answered.insert(id.as_str());
                }
                Message::System(_) | Message::User(_) | Message::Image { .. } => {}
            }
        }
        (calls.into_iter())
            .filter(|call| !answered.contains(call.id.as_str()))
            .map(|call| call.id.clone())
            .collect()
    }

    /// A tool's result for the call `id`: its report.
    pub(crate) fn result(&mut self, id: &str, report: &str) {
        self.push(Message::ToolResult {
            id: id.to_string(),
            content: shortened(report),
            failed: false,
        });
    }

    /// A picture the person attached to their message, named as they named it.
    pub(crate) fn attach(&mut self, name: &str, image: Image) {
        let caption = format!(
            "The person attached {name} ({}x{}).",
            image.width, image.height
        );
        // Kept for good: it is what the person asked about.
        self.push(Message::Image { caption, image });
    }

    /// The tokens the conversation takes, estimated.
    pub(crate) fn tokens(&self) -> u64 {
        self.sizes.iter().sum()
    }

    /// The tokens message `at` takes, estimated, as kept with it.
    pub(crate) fn size_at(&self, at: usize) -> u64 {
        self.sizes.get(at).copied().unwrap_or(0)
    }

    /// Replace message `at`'s output, or its calls' large arguments, by pointers (see `history`);
    /// how many were replaced.
    pub(crate) fn elide(&mut self, at: usize) -> usize {
        let elided = match self.messages.get_mut(at) {
            Some(Message::ToolResult { content, .. }) => {
                *content = fit::pointer(content.chars().count());
                1
            }
            Some(Message::Assistant { calls, .. }) => {
                let mut elided = 0;
                for call in calls {
                    // A text tool's arguments are the text itself.
                    let values: Vec<&mut Value> = match call.arguments.is_object() {
                        true => (call.arguments.as_object_mut().into_iter())
                            .flat_map(|map| map.values_mut())
                            .collect(),
                        false => vec![&mut call.arguments],
                    };
                    for value in values {
                        if let Some(text) = value.as_str().filter(|text| fit::worth(text)) {
                            *value = Value::String(fit::argument(text.chars().count()));
                            elided += 1;
                        }
                    }
                }
                elided
            }
            Some(Message::System(_) | Message::User(_) | Message::Image { .. }) | None => 0,
        };
        self.resized(at);
        elided
    }

    /// A call that did nothing useful, and why.
    pub(crate) fn failure(&mut self, id: &str, why: &str) {
        self.push(Message::ToolResult {
            id: id.to_string(),
            content: why.to_string(),
            failed: true,
        });
    }
}

/// `text` within `MAX_RESULT` characters: its head and tail, and how much was cut between them.
fn shortened(text: &str) -> String {
    let count = text.chars().count();
    if count <= MAX_RESULT {
        return text.to_string();
    }
    let keep = MAX_RESULT / 2;
    let head: String = text.chars().take(keep).collect();
    let tail: String = text.chars().skip(count - keep).collect();
    format!("{head}\n… {} characters cut …\n{tail}", count - 2 * keep)
}

#[cfg(test)]
#[path = "conversation_tests.rs"]
mod tests;
