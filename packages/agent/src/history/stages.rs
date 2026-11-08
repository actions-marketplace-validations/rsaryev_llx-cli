//! The pipeline's stages, least valuable first: the reasoning old turns carried back, old outputs
//! but those the app keeps, large arguments.

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
