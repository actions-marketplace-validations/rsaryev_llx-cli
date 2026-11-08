//! The pipeline's stages, least valuable first: the reasoning old turns carried back, outputs of
//! a call made again, old outputs but those the app keeps, large arguments.

use std::collections::{HashMap, HashSet};

use super::{Frame, Lasting, Pass, Shed, Stage, elide, large, pass, worth};
use crate::conversation::Conversation;
use contracts::chat::{Message, ToolCall};

/// The reasoning old turns carried back, as far as the provider's `shed` lets it go.
pub(super) struct Reasoning {
    pub(super) shed: Box<Shed>,
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

/// Outputs of a call made again later, the same tool with the same arguments (a file read twice,
/// `git status` run again): the newest copy says what the old one did, or more. Free of any rule
/// of a tool's (L5): only the call's name and arguments are compared.
pub(super) struct Repeats;

impl Stage for Repeats {
    fn name(&self) -> &'static str {
        "repeats"
    }

    fn run(&self, conversation: &mut Conversation, frame: &Frame) -> usize {
        let repeated = repeated(conversation.messages());
        elide(conversation, frame, |message| {
            matches!(message, Message::ToolResult { id, content, .. }
                if worth(content) && repeated.contains(id))
        })
    }
}

/// The ids of the outputs whose call was made again later, the same tool with the same arguments.
fn repeated(messages: &[Message]) -> HashSet<String> {
    let mut calls: HashMap<&str, &ToolCall> = HashMap::new();
    let mut newest: HashMap<(&str, String), &str> = HashMap::new();
    let mut older = HashSet::new();
    for message in messages {
        match message {
            Message::Assistant { calls: made, .. } => {
                calls.extend(made.iter().map(|call| (call.id.as_str(), call)));
            }
            Message::ToolResult { id, .. } => {
                if let Some(call) = calls.get(id.as_str()) {
                    let key = (call.name.as_str(), call.arguments.to_string());
                    if let Some(before) = newest.insert(key, id.as_str()) {
                        older.insert(before.to_string());
                    }
                }
            }
            Message::System(_) | Message::User(_) | Message::Image { .. } => {}
        }
    }
    older
}

/// Tool outputs, any worth eliding but those the app keeps.
pub(super) struct Outputs {
    pub(super) keep: Box<Lasting>,
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
pub(super) struct Arguments;

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
