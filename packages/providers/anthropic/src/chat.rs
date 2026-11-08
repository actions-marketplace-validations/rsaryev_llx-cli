//! The chat port over the Messages API: a request as the API takes it (`request.rs`), the reply
//! as a completion: its words, its calls, its signed thinking carried with the turn, and the usage
//! with what the prompt cache read and wrote.

use std::collections::HashSet;
use std::sync::{Mutex, PoisonError};

use contracts::catalog::Selection;
use contracts::chat::{Chat, ChatError, Completion, Ending, Request, ToolCall, Usage};
use contracts::tool::{Input, ToolSpec};
use serde_json::{Value, json};

use crate::client::{ApiError, Messages};
use crate::request::{BLOCKS, body, signature, signatures, words};

pub use crate::request::Thinking;

/// The beta the older models think between tool calls with, in budgeted (manual) thinking.
const INTERLEAVED: &str = "interleaved-thinking-2025-05-14";

/// A model `selection` names, over the Messages API `client` reaches, thinking as `thinking`
/// says it takes.
pub struct ChatModel {
    client: Messages,
    selection: Selection,
    thinking: Thinking,
    /// The thinking blocks the API refused as bound to a history that changed since (an elision,
    /// another set of tools), by signature: never sent again, so one change costs one request
    /// more, not one at every turn after it. Blocks written since stay valid and go back.
    forgotten: Mutex<HashSet<String>>,
}

impl ChatModel {
    pub fn new(client: Messages, selection: Selection, thinking: Thinking) -> Self {
        Self {
            client,
            selection,
            thinking,
            forgotten: Mutex::default(),
        }
    }

    fn forgotten(&self) -> HashSet<String> {
        let forgotten = self
            .forgotten
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        forgotten.clone()
    }
}

impl Chat for ChatModel {
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        let selection = self.selection.or_at(request.effort);
        let model = &selection.model().id;
        let sent = |thinking: Option<Thinking>, forgotten: &HashSet<String>| {
            let body = body(&selection, thinking, &request, forgotten);
            let beta = (thinking == Some(Thinking::Budget)).then_some(INTERLEAVED);
            (self.client.post(&body, request.deadline, beta), body)
        };
        let answered = match sent(Some(self.thinking), &self.forgotten()) {
            // Earlier thinking is bound to the history it was written in: the turn is asked once
            // more without the blocks this request sent, which are not sent again. A budgeted
            // model in a tool loop wants its last turn to start with thinking: it is asked with
            // none this once.
            (Err(ApiError::Http(400, message)), refused) if unbound(&message) => {
                log::info!(
                    "{} refused earlier thinking, sent without it",
                    self.client.name
                );
                let forgotten = {
                    let mut forgotten = self
                        .forgotten
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    forgotten.extend(signatures(&refused));
                    forgotten.clone()
                };
                let thinking = match self.thinking {
                    Thinking::Adaptive => Some(Thinking::Adaptive),
                    Thinking::Budget => None,
                };
                sent(thinking, &forgotten).0
            }
            (other, _) => other,
        };
        let (value, millis) = answered.map_err(|error| match error {
            ApiError::Invalid(message) => ChatError::Invalid(message),
            other @ (ApiError::Auth(_) | ApiError::Http(..) | ApiError::Network(_)) => {
                ChatError::Unavailable(format!("{} {other}", self.client.name))
            }
        })?;
        read(&value, model, millis, request.tools)
    }
}

/// Whether a refusal is of thinking blocks a changed history no longer matches.
fn unbound(message: &str) -> bool {
    message.contains("`thinking`") || message.contains("signature")
}

/// How a message ended, or why it is no answer.
fn ending(value: &Value) -> Result<Ending, ChatError> {
    match value["stop_reason"].as_str() {
        Some("max_tokens") => Ok(Ending::Cut),
        // Declined, even midway: what it wrote before is not an answer to keep.
        Some("refusal") => {
            let why = (value.pointer("/stop_details/explanation"))
                .and_then(Value::as_str)
                .unwrap_or("no reason given");
            Err(ChatError::Invalid(format!(
                "the model declined to answer: {why}"
            )))
        }
        Some("model_context_window_exceeded") => Err(ChatError::Invalid(
            "the conversation no longer fits the model's context window".into(),
        )),
        Some(_) => Ok(Ending::Whole),
        None => Err(ChatError::Invalid(
            "the reply ended before its message did".into(),
        )),
    }
}

/// The completion a message makes.
fn read(
    value: &Value,
    model: &str,
    millis: u64,
    tools: &[ToolSpec],
) -> Result<Completion, ChatError> {
    let content = (value["content"].as_array())
        .ok_or_else(|| ChatError::Invalid("no content in the message".into()))?;
    let ending = ending(value)?;
    let of = |kind: &'static str| content.iter().filter(move |block| block["type"] == kind);
    let thought: String = (of("thinking").filter_map(|block| block["thinking"].as_str()))
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    // The turn as written, with its signed thinking, for the next request to send back as it
    // came; a call's input is the conversation's to give then. A block the limit cut before its
    // signature is left out: unsigned, it would be refused.
    let layout: Vec<Value> = (content.iter())
        .filter_map(|block| match block["type"].as_str() {
            Some("tool_use") => Some(json!({"type": "tool_use", "id": block["id"]})),
            Some("text") => Some(json!({"type": "text", "text": block["text"]})),
            _ => signature(block).map(|_| block.clone()),
        })
        .collect();
    let signed = layout.iter().any(|block| signature(block).is_some());
    let mut calls: Vec<ToolCall> = of("tool_use")
        .map(|block| ToolCall {
            id: block["id"].as_str().unwrap_or_default().to_string(),
            name: block["name"].as_str().unwrap_or_default().to_string(),
            arguments: block["input"].clone(),
        })
        .collect();
    salvage(&mut calls, ending, tools);
    let tokens = |key: &str| value["usage"][key].as_u64().unwrap_or(0);
    let (read, written) = (
        tokens("cache_read_input_tokens"),
        tokens("cache_creation_input_tokens"),
    );
    let text = words(&layout);
    Ok(Completion {
        text: Some(text).filter(|text| !text.is_empty()),
        calls,
        carried: match signed {
            true => vec![json!({"model": model, BLOCKS: layout})],
            false => vec![],
        },
        thought: Some(thought).filter(|thought| !thought.is_empty()),
        usage: Usage {
            model: value["model"].as_str().unwrap_or(model).to_string(),
            cost: 0.0,
            millis,
            // The API counts apart what the cache read and wrote; the port counts them in.
            input_tokens: tokens("input_tokens") + read + written,
            cached_tokens: read,
            written_tokens: written,
            output_tokens: tokens("output_tokens"),
        },
        ending,
    })
}

/// Each call as the agent takes it: a patch's text out of its one argument. The last call of a
/// reply the limit cut off keeps what it wrote when its tool takes text (`apply_patch` writes the
/// files that are whole, B7); any other stays the raw text it is, and is not run.
fn salvage(calls: &mut [ToolCall], ending: Ending, tools: &[ToolSpec]) {
    let text_tool = |name: &str| {
        (tools.iter()).any(|spec| spec.name == name && matches!(spec.input, Input::Text(_)))
    };
    if ending == Ending::Cut
        && let Some(call) = calls.last_mut()
        && let Value::String(raw) = &call.arguments
        && text_tool(&call.name)
        && let Some(closed) = provider_kit::cut::closed(raw)
    {
        call.arguments = closed;
    }
    for call in calls.iter_mut() {
        if let Some(spec) = tools.iter().find(|spec| spec.name == call.name) {
            call.arguments = spec.input.from_function(call.arguments.take());
        }
    }
}

/// What of `kept`, a value a turn carried back, may go once the turn is old (the history's
/// `Shed`): its thinking blocks, as the API takes a turn without them but the last (which the
/// history never reaches); the turn's words and calls stay in their order. A value of another
/// provider's is not this one's to change.
pub fn shed(kept: &mut Value) -> bool {
    let Some(layout) = kept.get_mut(BLOCKS).and_then(Value::as_array_mut) else {
        return false;
    };
    let before = layout.len();
    layout.retain(|block| {
        !matches!(
            block["type"].as_str(),
            Some("thinking" | "redacted_thinking")
        )
    });
    layout.len() != before
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;
