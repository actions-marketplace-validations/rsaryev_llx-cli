//! A request's body in the Messages API's shape: the system prompt apart, the conversation as
//! user and assistant turns of content blocks, the tools, the level and the thinking, and the
//! marks that say where the prompt cache ends.

use contracts::catalog::{Effort, Selection};
use contracts::chat::{Calling, Message, Request, ToolCall};
use contracts::tool::{Image, ToolSpec};
use serde_json::{Value, json};

/// The most a reply may write, in tokens, or the model's own limit when it is smaller: room for a
/// whole file in one call, and a bound on its thinking.
const MAX_OUTPUT: u64 = 32_000;
/// Where `carried` keeps this wire's thinking blocks: another provider's has no such field.
pub(crate) const BLOCKS: &str = "anthropic";

/// How the model is told to think, as the catalog says it takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Thinking {
    /// It decides how much to think, at the level sent (`effort`): the current models.
    Adaptive,
    /// It thinks up to a number of tokens the level stands for: the older ones.
    Budget,
}

/// Whether the thinking blocks of earlier turns go back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Thoughts {
    Kept,
    /// Left out: the API refused them as bound to a history that changed since.
    Dropped,
}

/// The body for `request` to the model `selection` names, streamed.
pub(crate) fn body(
    selection: &Selection,
    thinking: Thinking,
    request: &Request,
    thoughts: Thoughts,
) -> Value {
    let model = &selection.model().id;
    let limit = match selection.model().output {
        0 => MAX_OUTPUT,
        own => own.min(MAX_OUTPUT),
    };
    let (system, mut messages) = conversation(request, model, thoughts);
    marked(&mut messages);
    let mut body = json!({"model": model, "max_tokens": limit, "stream": true});
    if let Some(system) = system {
        // The tools come before it in the prompt: one mark here caches both.
        body["system"] = json!([{"type": "text", "text": system, "cache_control": ephemeral()}]);
    }
    body["messages"] = Value::Array(messages);
    if !request.tools.is_empty() {
        body["tools"] = request.tools.iter().map(tool).collect();
        // The current models refuse a forced call; the request that wants one offers it alone
        // and asks for it in its last message.
        body["tool_choice"] = match request.calling {
            Calling::Any | Calling::Only(_) => json!({"type": "auto"}),
            Calling::Nothing => json!({"type": "none"}),
        };
    }
    if !selection.model().efforts.is_empty() {
        think(&mut body, thinking, selection.effort(), limit);
    }
    body
}

/// The level as the model takes it: the effort it decides its thinking by, or a budget of tokens.
fn think(body: &mut Value, thinking: Thinking, effort: Effort, limit: u64) {
    let budget = match effort {
        Effort::Off => {
            if thinking == Thinking::Adaptive {
                body["thinking"] = json!({"type": "disabled"});
            }
            return;
        }
        Effort::Minimal | Effort::Low => 2_048,
        Effort::Medium => 8_192,
        Effort::High | Effort::XHigh | Effort::Max | Effort::Ultra => 16_384,
    };
    match thinking {
        Thinking::Adaptive => {
            body["thinking"] = json!({"type": "adaptive"});
            body["output_config"] = json!({"effort": effort.wire_name()});
        }
        Thinking::Budget => {
            let budget = budget.min(limit / 2);
            body["thinking"] = json!({"type": "enabled", "budget_tokens": budget});
        }
    }
}

/// The system prompt, and the rest as alternating turns: a call's results and a picture after
/// them are one user turn, as the API wants a call's results right after it.
fn conversation(
    request: &Request,
    model: &str,
    thoughts: Thoughts,
) -> (Option<String>, Vec<Value>) {
    let mut system = None;
    let mut turns: Vec<Value> = Vec::new();
    for (at, message) in request.conversation.iter().enumerate() {
        let (role, blocks) = match message {
            Message::System(text) if at == 0 => {
                system = Some(text.clone());
                continue;
            }
            Message::System(text) | Message::User(text) => ("user", vec![text_block(text)]),
            Message::Image { caption, image } => ("user", picture(caption, image)),
            Message::ToolResult {
                id,
                content,
                failed,
            } => {
                let content = if content.is_empty() {
                    "(no output)"
                } else {
                    content
                };
                let result = json!({"type": "tool_result", "tool_use_id": id,
                    "content": content, "is_error": failed});
                ("user", vec![result])
            }
            Message::Assistant {
                text,
                calls,
                carried,
            } => (
                "assistant",
                said(
                    text.as_deref(),
                    calls,
                    carried,
                    model,
                    thoughts,
                    request.tools,
                ),
            ),
        };
        if blocks.is_empty() {
            continue;
        }
        match turns.last_mut() {
            Some(last) if last["role"] == role => {
                if let Some(content) = last["content"].as_array_mut() {
                    content.extend(blocks);
                }
            }
            _ => turns.push(json!({"role": role, "content": blocks})),
        }
    }
    (system, turns)
}

/// An assistant turn's blocks: its thinking first, as the model wrote it and signed it, then its
/// words, then its calls.
fn said(
    text: Option<&str>,
    calls: &[ToolCall],
    carried: &[Value],
    model: &str,
    thoughts: Thoughts,
    tools: &[ToolSpec],
) -> Vec<Value> {
    let mut blocks: Vec<Value> = Vec::new();
    if thoughts == Thoughts::Kept {
        let own = carried.iter().filter(|kept| kept["model"] == model);
        for kept in own.filter_map(|kept| kept[BLOCKS].as_array()) {
            blocks.extend(kept.iter().cloned());
        }
    }
    if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
        blocks.push(text_block(text));
    }
    for call in calls {
        // A patch's text goes back in the one argument it was offered with; a call cut off and
        // never run has no object to give, and goes back empty.
        let input = match tools.iter().find(|spec| spec.name == call.name) {
            Some(spec) => spec.input.to_function(&call.arguments),
            None => call.arguments.clone(),
        };
        let input = if input.is_object() { input } else { json!({}) };
        blocks.push(json!({"type": "tool_use", "id": call.id, "name": call.name, "input": input}));
    }
    blocks
}

/// A mark: the prompt up to here is written to the cache, or read from it.
fn ephemeral() -> Value {
    json!({"type": "ephemeral"})
}

fn text_block(text: &str) -> Value {
    let text = if text.is_empty() { "(empty)" } else { text };
    json!({"type": "text", "text": text})
}

fn picture(caption: &str, image: &Image) -> Vec<Value> {
    vec![
        text_block(caption),
        json!({"type": "image", "source": {"type": "base64",
            "media_type": image.media_type, "data": image.data}}),
    ]
}

fn tool(spec: &ToolSpec) -> Value {
    json!({"name": spec.name, "description": spec.description,
        "input_schema": spec.input.schema()})
}

/// The cache's marks, past the system prompt's: the first message (the context, the same for
/// every task in a workspace), the end of the user turn the previous request ended with (where
/// its prompt was written: read back whole however many blocks the turns since added) and the end
/// of the last turn (written for the next). Four marks at most, the system prompt's among them.
fn marked(turns: &mut [Value]) {
    let mark = |turn: Option<&mut Value>, first: bool| {
        let content = turn.and_then(|turn| turn["content"].as_array_mut());
        let block = match first {
            true => content.and_then(|content| content.first_mut()),
            false => content.and_then(|content| content.last_mut()),
        };
        if let Some(block) = block {
            block["cache_control"] = ephemeral();
        }
    };
    let Some(last) = turns.len().checked_sub(1) else {
        return;
    };
    let before = (0..last)
        .rev()
        .skip_while(|at| turns[*at]["role"] != "assistant")
        .find(|at| turns[*at]["role"] == "user");
    mark(turns.first_mut(), true);
    if let Some(before) = before {
        mark(turns.get_mut(before), false);
    }
    mark(turns.get_mut(last), false);
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;
