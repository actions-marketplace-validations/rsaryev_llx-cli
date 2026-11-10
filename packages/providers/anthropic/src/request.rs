//! A request's body in the Messages API's shape: the system prompt apart, the conversation as
//! user and assistant turns of content blocks, the tools, the level and the thinking, and the
//! marks that say where the prompt cache ends.

use std::collections::HashSet;

use contracts::catalog::{Effort, Selection};
use contracts::chat::{Calling, Message, Request, ToolCall};
use contracts::tool::{Image, ToolSpec};
use serde_json::{Value, json};

/// The most a reply may write, in tokens, or the model's own limit when it is smaller: room for
/// its thinking and the files it writes after it, at every level. At 32k a reply that thought long
/// was cut before its first call and was lost whole, some 150 s of a small model's writing.
const MAX_OUTPUT: u64 = 64_000;
/// Where `carried` keeps this wire's turn as the model wrote it: its signed thinking, its words
/// and its calls in their order. Another provider's has no such field.
pub(crate) const BLOCKS: &str = "anthropic";

/// How the model is told to think, as the catalog says it takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Thinking {
    /// It decides how much to think, at the level sent (`effort`): the current models.
    Adaptive,
    /// It thinks up to a number of tokens the level stands for: the older ones.
    Budget,
}

/// The body for `request` to the model `selection` names, streamed: it thinks as `thinking` says
/// (`None`: no thinking asked for), and the earlier thinking blocks `forgotten` names (by their
/// signature) stay out.
pub(crate) fn body(
    selection: &Selection,
    thinking: Option<Thinking>,
    request: &Request,
    forgotten: &HashSet<String>,
) -> Value {
    let model = &selection.model().id;
    let limit = match selection.model().output {
        0 => MAX_OUTPUT,
        own => own.min(MAX_OUTPUT),
    };
    let (system, mut messages) = conversation(request, model, forgotten);
    marked(&mut messages);
    let mut body = json!({"model": model, "max_tokens": limit, "stream": true});
    if let Some(system) = system {
        // The tools come before it in the prompt: one mark here caches both.
        body["system"] = json!([{"type": "text", "text": system, "cache_control": ephemeral()}]);
    }
    body["messages"] = Value::Array(messages);
    if !request.tools.is_empty() {
        body["tools"] = request.tools.iter().map(tool).collect();
        // The current models refuse a forced call: the request that wants one asks for it in
        // its last message, and `auto` keeps the messages' cache, which a changed choice drops.
        body["tool_choice"] = match request.calling {
            Calling::Any | Calling::Only(_) => json!({"type": "auto"}),
            Calling::Nothing => json!({"type": "none"}),
        };
    }
    if let Some(thinking) = thinking
        && !selection.model().efforts.is_empty()
    {
        think(&mut body, thinking, selection.effort(), limit);
    }
    body
}

/// The signature of each thinking block `body` sends: what a refusal of them names.
pub(crate) fn signatures(body: &Value) -> Vec<String> {
    let turns = body["messages"].as_array().into_iter().flatten();
    let blocks = turns.flat_map(|turn| turn["content"].as_array().into_iter().flatten());
    blocks.filter_map(signature).map(str::to_string).collect()
}

/// What a thinking block is known by: its signature, or a redacted one's sealed data.
pub(crate) fn signature(block: &Value) -> Option<&str> {
    match block["type"].as_str() {
        Some("thinking") => block["signature"].as_str(),
        Some("redacted_thinking") => block["data"].as_str(),
        _ => None,
    }
    .filter(|known| !known.is_empty())
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
    forgotten: &HashSet<String>,
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
                    forgotten,
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

/// An assistant turn's blocks: as the model wrote them (its signed thinking, its words and its
/// calls in their order, which the API checks thinking against), its calls as the conversation
/// holds them now; else, for another model's turn or words changed since, its words, then its
/// calls.
fn said(
    text: Option<&str>,
    calls: &[ToolCall],
    carried: &[Value],
    model: &str,
    forgotten: &HashSet<String>,
    tools: &[ToolSpec],
) -> Vec<Value> {
    let call_block = |call: &ToolCall| {
        // A patch's text goes back in the one argument it was offered with; a call cut off and
        // never run has no object to give, and goes back empty.
        let input = match tools.iter().find(|spec| spec.name == call.name) {
            Some(spec) => spec.input.to_function(&call.arguments),
            None => call.arguments.clone(),
        };
        let input = if input.is_object() { input } else { json!({}) };
        json!({"type": "tool_use", "id": call.id, "name": call.name, "input": input})
    };
    let mut blocks: Vec<Value> = Vec::new();
    let written = (carried.iter())
        .find(|kept| kept["model"] == model)
        .and_then(|kept| kept[BLOCKS].as_array())
        .filter(|layout| words(layout) == text.unwrap_or_default());
    let mut placed = HashSet::new();
    for block in written.into_iter().flatten() {
        match block["type"].as_str() {
            Some("tool_use") => {
                let id = block["id"].as_str().unwrap_or_default();
                if let Some(call) = calls.iter().find(|call| call.id == id) {
                    blocks.push(call_block(call));
                    placed.insert(id);
                }
            }
            Some("text") => blocks.push(block.clone()),
            _ => {
                if signature(block).is_some_and(|known| !forgotten.contains(known)) {
                    blocks.push(block.clone());
                }
            }
        }
    }
    if written.is_none()
        && let Some(text) = text.filter(|text| !text.trim().is_empty())
    {
        blocks.push(text_block(text));
    }
    for call in calls
        .iter()
        .filter(|call| !placed.contains(call.id.as_str()))
    {
        blocks.push(call_block(call));
    }
    blocks
}

/// A turn's words as the completion gives them: its text blocks, joined.
pub(crate) fn words(blocks: &[Value]) -> String {
    let texts = blocks.iter().filter(|block| block["type"] == "text");
    let texts: Vec<&str> = texts.filter_map(|block| block["text"].as_str()).collect();
    texts.join("\n").trim().to_string()
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

/// A tool as the API takes it. Its input is streamed as the model writes it
/// (`eager_input_streaming`): else the API holds a string back until it is whole, and a call the
/// output limit cut arrives empty, with nothing of it to carry out (B7).
fn tool(spec: &ToolSpec) -> Value {
    json!({"name": spec.name, "description": spec.description,
        "input_schema": spec.input.schema(), "eager_input_streaming": true})
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
