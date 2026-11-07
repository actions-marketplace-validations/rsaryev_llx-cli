//! A streamed message (`stream: true`), put back together as the message the API sends whole:
//! each content block from its start and deltas, the stop reason and the usage. Streamed, a reply
//! the output limit cut off keeps what its last call wrote: the call's input stays the text it is
//! when it does not parse, for the tool that can tell what of it is whole (B7).

use serde_json::{Map, Value, json};

/// The message `body`'s events make, as one sent whole; `None` when it holds no event (it is not
/// a stream). An error the host reports midway is the answer: `{"error": …}`, with
/// `"written": true` when words or calls came before it (they were billed).
pub(crate) fn assembled(body: &str) -> Option<Value> {
    let mut events = provider_kit::sse::events(body).peekable();
    events.peek()?;
    let mut whole = Whole::default();
    for mut event in events {
        if event["type"] == "error" {
            let error = event["error"].take();
            return Some(match whole.blocks.is_empty() {
                true => json!({ "error": error }),
                false => json!({ "error": error, "written": true }),
            });
        }
        whole.take(&mut event);
    }
    Some(whole.message())
}

/// What the events said so far.
#[derive(Default)]
struct Whole {
    model: Value,
    usage: Map<String, Value>,
    stop: Value,
    /// Each block as it started, its deltas added: a call's input as the text that came.
    blocks: Vec<(Value, String)>,
}

impl Whole {
    fn take(&mut self, event: &mut Value) {
        match event["type"].as_str() {
            Some("message_start") => {
                self.model = event["message"]["model"].take();
                self.add_usage(&event["message"]["usage"]);
            }
            Some("content_block_start") => {
                self.blocks
                    .push((event["content_block"].take(), String::new()));
            }
            Some("content_block_delta") => self.delta(&event["delta"]),
            Some("message_delta") => {
                self.stop = event["delta"]["stop_reason"].take();
                self.add_usage(&event["usage"]);
            }
            // `ping`, `content_block_stop`, `message_stop` and kinds added later say nothing new.
            _ => {}
        }
    }

    /// A delta adds to the block the stream is writing, the last one started.
    fn delta(&mut self, delta: &Value) {
        let Some((block, input)) = self.blocks.last_mut() else {
            return;
        };
        let text = |key: &str| delta[key].as_str().unwrap_or_default();
        match delta["type"].as_str() {
            Some("text_delta") => append(&mut block["text"], text("text")),
            Some("thinking_delta") => append(&mut block["thinking"], text("thinking")),
            Some("signature_delta") => append(&mut block["signature"], text("signature")),
            Some("input_json_delta") => input.push_str(text("partial_json")),
            _ => {}
        }
    }

    /// Later counts stand for the same fields: the last delta's output is the whole output.
    fn add_usage(&mut self, usage: &Value) {
        if let Some(fields) = usage.as_object() {
            for (key, value) in fields.iter().filter(|(_, value)| !value.is_null()) {
                self.usage.insert(key.clone(), value.clone());
            }
        }
    }

    fn message(self) -> Value {
        let content: Vec<Value> = (self.blocks.into_iter())
            .map(|(mut block, input)| {
                if block["type"] == "tool_use" && !input.is_empty() {
                    // Input that does not parse is a call the limit cut off: kept as its text.
                    block["input"] = serde_json::from_str(&input).unwrap_or(Value::String(input));
                }
                block
            })
            .collect();
        json!({
            "model": self.model,
            "content": content,
            "stop_reason": self.stop,
            "usage": self.usage,
        })
    }
}

/// `more` after the text in `field`; a field with no text yet (absent, `null`) starts with it.
fn append(field: &mut Value, more: &str) {
    if let Value::String(text) = field {
        text.push_str(more);
    } else {
        *field = Value::String(more.to_string());
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
