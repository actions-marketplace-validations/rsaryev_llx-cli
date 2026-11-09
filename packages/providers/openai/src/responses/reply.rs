//! A streamed reply read: the completion its finished items make, its usage, and a failure it
//! reports, asked again when a wait may mend it.

use contracts::chat::{ChatError, Completion, Ending, ToolCall, Usage};
use provider_kit::sse;
use serde_json::{Value, json};

use super::offer::custom_call;

/// The failure a reply reports before any of it was written, when a wait may mend it: an
/// overload or a rate limit of a moment, or a code not known (as Codex asks again).
pub(super) fn failed_for_load(body: &str) -> Option<ChatError> {
    const LASTING: [&str; 5] = [
        "context_length_exceeded",
        "insufficient_quota",
        "usage_not_included",
        "invalid_prompt",
        "usage_limit_reached",
    ];
    for event in sse::events(body) {
        match event["type"].as_str().unwrap_or("") {
            "response.failed" | "error" => {
                let error = event.pointer("/response/error").unwrap_or(&event);
                let code = error["code"].as_str().unwrap_or("");
                let message = error["message"].as_str().unwrap_or("no message");
                return (!LASTING.contains(&code))
                    .then(|| ChatError::Unavailable(format!("ChatGPT failed: {message}")));
            }
            // Words or an item written: billed, never asked again.
            kind if kind.starts_with("response.output") => return None,
            _ => {}
        }
    }
    None
}

/// The completion in a streamed reply: its finished output items and the usage it ended with.
pub(super) fn read(body: &str, model: &str) -> Result<Completion, ChatError> {
    let mut completion = Completion::default();
    let mut said: Vec<String> = vec![];
    let mut ended = false;
    // Each item moved, not copied, into what goes back: `json!` of a value serializes a second
    // tree of it, and a sealed reasoning is kilobytes.
    let carry = |item: Value| {
        let mut kept = json!({"model": model});
        kept["item"] = item;
        kept
    };
    for mut event in sse::events(body) {
        match event["type"].as_str().unwrap_or("") {
            "response.output_item.done" => {
                let mut item = event["item"].take();
                match item["type"].as_str() {
                    Some("message") => {
                        said.extend(
                            item["content"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter(|part| part["type"] == "output_text")
                                .filter_map(|part| part["text"].as_str().map(str::to_string)),
                        );
                        // Its phase goes back with it: a note beside calls is not an answer.
                        // Without its id: nothing is stored at the backend to name.
                        if item["phase"].is_string() {
                            if let Some(fields) = item.as_object_mut() {
                                fields.remove("id");
                            }
                            completion.carried.push(carry(item));
                        }
                    }
                    Some("function_call") => completion.calls.extend(call(&item)),
                    // A tool of free text offered as a custom one: its input is the text whole.
                    Some("custom_tool_call") => completion.calls.extend(custom_call(&item)),
                    // Nothing is stored at the backend: only a sealed reasoning can go back, and
                    // only to the model that sealed it.
                    Some("reasoning") if item["encrypted_content"].is_string() => {
                        completion.carried.push(carry(item));
                    }
                    _ => {}
                }
            }
            "response.completed" => {
                completion.usage = usage(&event["response"], model);
                ended = true;
            }
            // Cut at the output limit, the reply so far stands and the agent goes on; refused by
            // the content filter, there is nothing to go on from.
            "response.incomplete" => {
                if event.pointer("/response/incomplete_details/reason")
                    == Some(&json!("content_filter"))
                {
                    return Err(ChatError::Invalid(
                        "ChatGPT withheld the reply (content filter)".into(),
                    ));
                }
                completion.usage = usage(&event["response"], model);
                completion.ending = Ending::Cut;
                ended = true;
            }
            "response.failed" | "error" => {
                let error = event.pointer("/response/error").unwrap_or(&event);
                let message = error["message"].as_str().unwrap_or("no message");
                let window = match error["code"].as_str() {
                    Some("context_length_exceeded") => {
                        " (the conversation is past the model's window)"
                    }
                    _ => "",
                };
                return Err(ChatError::Unavailable(format!(
                    "ChatGPT failed: {message}{window}"
                )));
            }
            _ => {}
        }
    }
    if !ended {
        return Err(ChatError::Invalid(
            "the reply ended before it was complete".into(),
        ));
    }
    let said = said.join("\n");
    completion.text = Some(said.trim().to_string()).filter(|text| !text.is_empty());
    Ok(completion)
}

fn call(item: &Value) -> Option<ToolCall> {
    let raw = item["arguments"].as_str().unwrap_or("{}");
    Some(ToolCall {
        id: item["call_id"].as_str()?.to_string(),
        name: item["name"].as_str()?.to_string(),
        // Arguments that are not JSON stay as their text: the agent tells the model they were
        // incomplete instead of running the tool with none.
        arguments: serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string())),
    })
}

/// What the reply took. A ChatGPT plan is paid by subscription: no cost per call.
fn usage(response: &Value, model: &str) -> Usage {
    let tokens = |pointer: &str| {
        response
            .pointer(pointer)
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    Usage {
        model: response["model"].as_str().unwrap_or(model).to_string(),
        cost: 0.0,
        millis: 0,
        input_tokens: tokens("/usage/input_tokens"),
        cached_tokens: tokens("/usage/input_tokens_details/cached_tokens"),
        written_tokens: 0,
        output_tokens: tokens("/usage/output_tokens"),
    }
}
