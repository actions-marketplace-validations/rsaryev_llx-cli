//! A streamed chat completion (`stream: true`), put back together as the completion a host sends
//! whole: every event's deltas, in order. Streamed, a reply the output limit cut off keeps what
//! its last call wrote; sent whole, that call comes back with `{}` and its text is lost (B7).

use serde_json::{Map, Value, json};

use crate::chat::THOUGHT;

/// The completion `body`'s events make, as one sent whole; `None` when it holds no event (it is
/// not a stream). An error the host reports midway is the answer: `{"error": …}`.
pub(crate) fn assembled(body: &str) -> Option<Value> {
    let mut events = provider_kit::sse::events(body).peekable();
    events.peek()?;
    let mut whole = Whole::default();
    for mut event in events {
        if let Some(error) = event.get_mut("error") {
            return Some(json!({ "error": error.take() }));
        }
        whole.take(&event);
    }
    Some(whole.completion())
}

/// What the events said so far.
#[derive(Default)]
struct Whole {
    model: Option<Value>,
    provider: Option<Value>,
    content: String,
    /// Each reasoning field's text, by field, in the order they came.
    thought: Vec<(&'static str, String)>,
    /// The reasoning's parts, each whole again (`reasoning_details`).
    details: Vec<Value>,
    calls: Vec<Call>,
    finish: Option<Value>,
    usage: Option<Value>,
}

/// One call as its deltas built it, at its index.
#[derive(Default)]
struct Call {
    id: String,
    name: String,
    arguments: String,
}

impl Whole {
    fn take(&mut self, event: &Value) {
        for (field, kept) in [("model", &mut self.model), ("provider", &mut self.provider)] {
            if kept.is_none()
                && let Some(value) = event.get(field).filter(|value| value.is_string())
            {
                *kept = Some(value.clone());
            }
        }
        if let Some(usage) = event.get("usage").filter(|usage| usage.is_object()) {
            self.usage = Some(usage.clone());
        }
        let Some(choice) = event.pointer("/choices/0") else {
            return;
        };
        if let Some(finish) = choice.get("finish_reason").filter(|f| f.is_string()) {
            self.finish = Some(finish.clone());
        }
        let delta = &choice["delta"];
        self.content
            .push_str(delta["content"].as_str().unwrap_or(""));
        for field in THOUGHT {
            if let Some(text) = delta[field].as_str().filter(|text| !text.is_empty()) {
                self.thought(field).push_str(text);
            }
        }
        for part in delta["reasoning_details"].as_array().into_iter().flatten() {
            self.detail(part);
        }
        for call in delta["tool_calls"].as_array().into_iter().flatten() {
            self.call(call);
        }
    }

    fn thought(&mut self, field: &'static str) -> &mut String {
        let at = match self.thought.iter().position(|(name, _)| *name == field) {
            Some(at) => at,
            None => {
                self.thought.push((field, String::new()));
                self.thought.len() - 1
            }
        };
        &mut self.thought[at].1
    }

    /// A part of the reasoning: the text of one with its index (and kind) goes on that one's,
    /// its other fields (the signature that comes last) are set on it.
    fn detail(&mut self, part: &Value) {
        let Some(fields) = part.as_object() else {
            return;
        };
        let same =
            |kept: &&mut Value| kept["index"] == part["index"] && kept["type"] == part["type"];
        let Some(kept) = self
            .details
            .iter_mut()
            .find(same)
            .and_then(Value::as_object_mut)
        else {
            self.details.push(part.clone());
            return;
        };
        for (name, value) in fields {
            match (kept.get_mut(name), value) {
                (Some(Value::String(text)), Value::String(more))
                    if matches!(name.as_str(), "text" | "summary" | "data") =>
                {
                    text.push_str(more);
                }
                _ => {
                    kept.insert(name.clone(), value.clone());
                }
            }
        }
    }

    fn call(&mut self, delta: &Value) {
        let at = (delta["index"].as_u64())
            .and_then(|index| usize::try_from(index).ok())
            .unwrap_or_else(|| self.calls.len().saturating_sub(1));
        while self.calls.len() <= at {
            self.calls.push(Call::default());
        }
        let call = &mut self.calls[at];
        if let Some(id) = delta["id"].as_str() {
            call.id = id.to_string();
        }
        let function = &delta["function"];
        if let Some(name) = function["name"].as_str() {
            call.name.push_str(name);
        }
        call.arguments
            .push_str(function["arguments"].as_str().unwrap_or(""));
    }

    fn completion(self) -> Value {
        let mut message = Map::new();
        message.insert("role".into(), json!("assistant"));
        let content = Some(self.content).filter(|content| !content.is_empty());
        message.insert("content".into(), json!(content));
        for (field, text) in self.thought {
            message.insert(field.into(), json!(text));
        }
        if !self.details.is_empty() {
            message.insert("reasoning_details".into(), json!(self.details));
        }
        if !self.calls.is_empty() {
            let calls: Vec<Value> = (self.calls.into_iter())
                .map(|call| {
                    json!({"id": call.id, "type": "function",
                        "function": {"name": call.name, "arguments": call.arguments}})
                })
                .collect();
            message.insert("tool_calls".into(), json!(calls));
        }
        let mut whole = json!({"choices": [{"index": 0, "message": message,
            "finish_reason": self.finish}]});
        for (field, value) in [
            ("model", self.model),
            ("provider", self.provider),
            ("usage", self.usage),
        ] {
            if let Some(value) = value {
                whole[field] = value;
            }
        }
        whole
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
