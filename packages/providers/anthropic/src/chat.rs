//! The chat port over the Messages API: a request as the API takes it (`request.rs`), the reply
//! as a completion: its words, its calls, its signed thinking carried with the turn, and the usage
//! with what the prompt cache read and wrote.

use contracts::catalog::Selection;
use contracts::chat::{Chat, ChatError, Completion, Ending, Request, ToolCall, Usage};
use contracts::tool::{Input, ToolSpec};
use serde_json::{Value, json};

use crate::client::{ApiError, Messages};
use crate::request::{BLOCKS, Thoughts, body};

pub use crate::request::Thinking;

/// A model `selection` names, over the Messages API `client` reaches, thinking as `thinking`
/// says it takes.
pub struct ChatModel {
    pub client: Messages,
    pub selection: Selection,
    pub thinking: Thinking,
}

impl Chat for ChatModel {
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        let selection = self.selection.or_at(request.effort);
        let model = &selection.model().id;
        let sent = |thoughts| {
            let body = body(&selection, self.thinking, &request, thoughts);
            self.client.post(&body, request.deadline)
        };
        let answered = match sent(Thoughts::Kept) {
            // Earlier thinking is bound to the history it was written in: a history the agent
            // changed since (an elision, another set of tools) has it refused. The turn is asked
            // once more without it, as the API says to.
            Err(ApiError::Http(400, message)) if unbound(&message) => {
                log::info!(
                    "{} refused earlier thinking, sent without it",
                    self.client.name
                );
                sent(Thoughts::Dropped)
            }
            other => other,
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

/// The completion a message makes.
fn read(
    value: &Value,
    model: &str,
    millis: u64,
    tools: &[ToolSpec],
) -> Result<Completion, ChatError> {
    let content = (value["content"].as_array())
        .ok_or_else(|| ChatError::Invalid("no content in the message".into()))?;
    let of = |kind: &'static str| content.iter().filter(move |block| block["type"] == kind);
    let ending = match value["stop_reason"].as_str() {
        Some("max_tokens") => Ending::Cut,
        Some("refusal") if content.is_empty() => {
            let why = value
                .pointer("/stop_details/explanation")
                .and_then(Value::as_str);
            return Err(ChatError::Invalid(format!(
                "the model declined to answer: {}",
                why.unwrap_or("no reason given")
            )));
        }
        _ => Ending::Whole,
    };
    let text: String = (of("text").filter_map(|block| block["text"].as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    let thought: String = (of("thinking").filter_map(|block| block["thinking"].as_str()))
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let signed: Vec<Value> = (content.iter())
        .filter(|block| block["type"] == "thinking" || block["type"] == "redacted_thinking")
        .cloned()
        .collect();
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
    Ok(Completion {
        text: Some(text.trim().to_string()).filter(|text| !text.is_empty()),
        calls,
        carried: match signed.is_empty() {
            true => vec![],
            false => vec![json!({"model": model, BLOCKS: signed})],
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

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;
