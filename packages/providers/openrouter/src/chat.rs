//! The chat port over OpenAI's chat completions, with tool calling: OpenRouter's models, and any
//! provider's that speaks the same wire. OpenRouter takes its own `reasoning` object, a usage
//! report with the cost, a session per conversation and prompt-cache marks for the models that
//! take them, and streams its reply; the others take `reasoning_effort`, and OpenAI's own API a
//! prompt cache's key per conversation and its own name for the output limit.

use contracts::catalog::Selection;
use contracts::chat::{
    Calling, Chat, ChatError, Completion, Ending, Message, Request, ToolCall, Usage,
};
use contracts::tool::{Image, Input, ToolSpec};
use serde_json::{Value, json};

use provider_kit::failure::ApiError;

use crate::client::{OpenRouter, Wire};
use crate::level::{self, Switch};

/// A model `selection` names, over the chat completions `client` reaches.
pub struct ChatModel {
    pub client: OpenRouter,
    pub selection: Selection,
    /// How its level goes out where the host takes no `reasoning` object of OpenRouter's.
    pub switch: Switch,
}

impl Chat for ChatModel {
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        let selection = self.selection.or_at(request.effort);
        let model = &selection.model().id;
        let body = body(self.client.wire(), &selection, &request, self.switch);
        let (value, millis) = self
            .client
            .asked("/chat/completions", body, request.deadline)
            .map_err(|error| match error {
                ApiError::Invalid(message) => ChatError::Invalid(message),
                other @ (ApiError::Auth(_) | ApiError::Http(..) | ApiError::Network(_)) => {
                    ChatError::Unavailable(format!("{} {other}", self.client.name))
                }
            })?;
        Ok(arrived(read(&value, model, millis)?, request.tools))
    }
}

/// A reply's calls as the agent takes them. The last of a reply the output limit cut off keeps
/// what it wrote when its tool takes text (`apply_patch` writes the files that are whole, B7); any
/// other stays the raw text it is, and is not run.
fn arrived(mut completion: Completion, tools: &[ToolSpec]) -> Completion {
    if completion.ending == Ending::Cut
        && let Some(call) = completion.calls.last_mut()
        && let Value::String(raw) = &call.arguments
        && (tools.iter()).any(|spec| spec.name == call.name && matches!(spec.input, Input::Text(_)))
        && let Some(closed) = provider_kit::cut::closed(raw)
    {
        call.arguments = closed;
    }
    completion.calls = (completion.calls.into_iter())
        .map(|call| incoming(call, tools))
        .collect();
    completion
}

/// A call the model made, as the agent takes it: a patch's text taken out of its one argument.
fn incoming(mut call: ToolCall, tools: &[ToolSpec]) -> ToolCall {
    if let Some(spec) = tools.iter().find(|spec| spec.name == call.name) {
        call.arguments = spec.input.from_function(call.arguments);
    }
    call
}

/// The completion in a reply: its text, its calls, the reasoning that goes back with it, and the
/// usage.
fn read(value: &Value, model: &str, millis: u64) -> Result<Completion, ChatError> {
    let message = value
        .pointer("/choices/0/message")
        .ok_or_else(|| ChatError::Invalid("no message in the completion".into()))?;
    // Who served it (OpenRouter routes a model to one of its hosts) and the calls as they came:
    // what a call that reached a tool malformed is traced back to.
    log::debug!(
        "{} served {}: {}",
        value["provider"].as_str().unwrap_or("the host"),
        value["model"].as_str().unwrap_or(model),
        message.get("tool_calls").unwrap_or(&Value::Null)
    );
    let calls = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|calls| calls.iter().filter_map(read_call).collect())
        .unwrap_or_default();
    let content = message.get("content").unwrap_or(&Value::Null);
    let text = (content.as_str().map(str::to_string))
        .or_else(|| crate::carry::words(content))
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());
    let finish = value
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str);
    if finish == Some("content_filter") {
        return Err(ChatError::Invalid(
            "the host withheld the reply (content filter)".into(),
        ));
    }
    let tokens = |pointer: &str| value.pointer(pointer).and_then(Value::as_u64).unwrap_or(0);
    // The first place that says some: a host may say zero in one and the count in another.
    let said = |places: &[&str]| (places.iter().map(|p| tokens(p)).find(|n| *n > 0)).unwrap_or(0);
    Ok(Completion {
        text,
        calls,
        carried: crate::carry::carried(message, model).into_iter().collect(),
        thought: readable(message),
        usage: Usage {
            model: value["model"].as_str().unwrap_or(model).to_string(),
            cost: cost(value),
            millis,
            input_tokens: tokens("/usage/prompt_tokens"),
            cached_tokens: said(&CACHE_READ),
            written_tokens: said(&CACHE_WRITTEN),
            output_tokens: tokens("/usage/completion_tokens"),
        },
        // `length`: the host's output limit stopped it, a call's arguments maybe midway.
        ending: match finish {
            Some("length") => Ending::Cut,
            _ => Ending::Whole,
        },
    })
}

/// What a reply cost: OpenRouter's charge, and on the person's own provider key (BYOK) what that
/// provider charged too, which OpenRouter's `cost` leaves out.
fn cost(value: &Value) -> f64 {
    let at = |pointer: &str| {
        value
            .pointer(pointer)
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    let upstream = match value.pointer("/usage/is_byok") == Some(&Value::Bool(true)) {
        true => at("/usage/cost_details/upstream_inference_cost"),
        false => 0.0,
    };
    at("/usage/cost") + upstream
}

/// Where a host says what a request read from its prompt cache: where most do, at the top of the
/// usage (Together's), as DeepSeek's own API does, as a relay passing on Anthropic's counts
/// does, and in llama.cpp's own timings.
const CACHE_READ: [&str; 5] = [
    "/usage/prompt_tokens_details/cached_tokens",
    "/usage/cached_tokens",
    "/usage/prompt_cache_hit_tokens",
    "/usage/cache_read_input_tokens",
    "/timings/cache_n",
];

/// Where a host says what a request wrote to its prompt cache: OpenRouter's place, Anthropic's.
const CACHE_WRITTEN: [&str; 2] = [
    "/usage/prompt_tokens_details/cache_write_tokens",
    "/usage/cache_creation_input_tokens",
];

/// The fields a host says its model's reasoning in, the first that holds some taken: a host may
/// say it twice, in two of them.
pub(crate) const THOUGHT: [&str; 3] = ["reasoning_content", "reasoning", "reasoning_text"];

/// The reasoning a message holds as text, from the first field that has some: none when it is only
/// sealed.
fn readable(message: &Value) -> Option<String> {
    THOUGHT.iter().find_map(|field| {
        let text = message.get(*field)?.as_str()?;
        (!text.trim().is_empty()).then(|| text.to_string())
    })
}

/// The request's body: OpenRouter's own fields, OpenAI's, or the plain ones any host takes.
fn body(host: Wire, selection: &Selection, request: &Request, switch: Switch) -> Value {
    let model = &selection.model().id;
    let messages: Vec<Value> = (request.conversation.iter())
        .map(|message| wire(message, model, request.tools))
        .collect();
    let limit = level::output(selection);
    // OpenAI's reasoning models refuse `max_tokens`; every chat model of its takes this one.
    let cap = match host {
        Wire::OpenAi => "max_completion_tokens",
        Wire::OpenRouter | Wire::Plain | Wire::Local => "max_tokens",
    };
    let mut body = json!({"model": model, cap: limit});
    // Only a model that takes levels is sent one: another may refuse the field.
    let levels = !selection.model().efforts.is_empty();
    // A plain host serving such a model (a proxy, a router) takes the marks as OpenRouter does;
    // one that refuses them is asked again without (`mend`).
    let marked = matches!(host, Wire::OpenRouter | Wire::Plain) && selection.model().marks;
    // Moved, not copied: `json!` of a value serializes a second tree of it.
    body["messages"] = Value::Array(if marked { cached(messages) } else { messages });
    match host {
        Wire::OpenRouter => {
            if levels {
                body["reasoning"] = level::reasoning(selection.effort());
            }
            body["usage"] = json!({"include": true});
            // One session a conversation: OpenRouter's logs show its requests together, and it
            // routes them to the host that holds its cache.
            body["session_id"] = json!(session(request.conversation));
        }
        Wire::OpenAi | Wire::Plain | Wire::Local => {
            if levels {
                level::switched(&mut body, switch, selection.effort());
            }
        }
    }
    // Streamed: a reply the output limit cuts off keeps what its last call wrote (B7). A local
    // host says its usage only in a stream asked for it.
    if matches!(host, Wire::OpenRouter | Wire::Local) {
        body["stream"] = json!(true);
    }
    if host == Wire::Local {
        body["stream_options"] = json!({"include_usage": true});
    }
    // OpenAI routes by the key: one per conversation sends its requests to where its own start
    // is cached, instead of every session to one machine.
    if host == Wire::OpenAi {
        body["prompt_cache_key"] = json!(session(request.conversation));
    }
    if !request.tools.is_empty() {
        body["tools"] = request.tools.iter().map(function).collect();
        body["tool_choice"] = match request.calling {
            Calling::Any => json!("auto"),
            Calling::Nothing => json!("none"),
            // Forced where OpenAI's own API takes it; elsewhere `auto`, the call asked for in
            // words (hosts refuse a named choice while their model thinks, or take only `auto`).
            Calling::Only(name) if host == Wire::OpenAi => {
                json!({"type": "function", "function": {"name": name}})
            }
            Calling::Only(_) => json!("auto"),
        };
        // Several calls in one turn: independent reads and searches cost one turn, not many.
        // The agent runs together only the calls whose tools allow it, the rest in order.
        body["parallel_tool_calls"] = json!(true);
    }
    body
}

/// The session a conversation is: its first words (the session's context and first goal, or an
/// agent's brief), the same at every later request of it, a follow-up's too. OpenRouter's
/// `session_id`, and OpenAI's prompt cache's key.
fn session(conversation: &[Message]) -> String {
    provider_kit::cache::key(conversation.iter().filter_map(Message::said).take(2))
}

/// A message in the chat-completions wire format, for `model`: the reasoning another model wrote
/// stays out.
fn wire(message: &Message, model: &str, tools: &[ToolSpec]) -> Value {
    match message {
        Message::System(text) => json!({"role": "system", "content": text}),
        Message::User(text) => json!({"role": "user", "content": text}),
        Message::Assistant {
            text,
            calls,
            carried,
        } => {
            let mut out = json!({"role": "assistant", "content": text});
            if !calls.is_empty() {
                out["tool_calls"] = calls
                    .iter()
                    .map(|call| {
                        // A patch's text goes back in the one argument it was offered with.
                        let arguments = match tools.iter().find(|spec| spec.name == call.name) {
                            Some(spec) => spec.input.to_function(&call.arguments),
                            None => call.arguments.clone(),
                        };
                        json!({"id": call.id, "type": "function", "function": {
                            "name": call.name, "arguments": arguments.to_string()}})
                    })
                    .collect();
            }
            crate::carry::restore(&mut out, carried, model);
            out
        }
        // The wire has no failure flag for a tool result: the text says it.
        Message::ToolResult {
            id,
            content,
            failed,
        } => {
            // Borrowed when it did not fail: every request sends every result again.
            let content: &str = match failed {
                true => &format!("Error: {content}"),
                false => content,
            };
            json!({"role": "tool", "tool_call_id": id, "content": content})
        }
        Message::Image { caption, image } => picture(caption, image),
    }
}

/// A picture as a user message with a text part and an image part: a tool message carries text
/// only, so it follows the call's result instead.
fn picture(caption: &str, image: &Image) -> Value {
    let url = format!("data:{};base64,{}", image.media_type, image.data);
    json!({"role": "user", "content": [
        {"type": "text", "text": caption},
        {"type": "image_url", "image_url": {"url": url}},
    ]})
}

fn function(spec: &ToolSpec) -> Value {
    json!({"type": "function", "function": {
        "name": spec.name, "description": spec.description, "parameters": spec.input.schema()}})
}

/// The conversation with prompt-cache breakpoints on the system prompt, the context after it and
/// the newest message, so each turn re-reads only what was added since the last one.
fn cached(mut messages: Vec<Value>) -> Vec<Value> {
    let mark = |message: &mut Value| {
        let content = &mut message["content"];
        if content.is_string() {
            // Moved, not copied: the system prompt and the context are the request's longest.
            let mut part = json!({"type": "text", "cache_control": {"type": "ephemeral"}});
            part["text"] = content.take();
            *content = Value::Array(vec![part]);
        } else if let Some(last) = content.as_array_mut().and_then(|p| p.last_mut()) {
            // Already in parts (a picture): the mark goes on the last one.
            last["cache_control"] = json!({"type": "ephemeral"});
        }
    };
    if let Some(system) = messages.first_mut().filter(|m| m["role"] == "system") {
        mark(system);
    }
    // The message after the system prompt is the context (the environment, the instructions):
    // the same for every task in a workspace, so a new session reads it from the cache too.
    if messages.len() > 2
        && messages[0]["role"] == "system"
        && let Some(context) = messages.get_mut(1).filter(|m| m["role"] == "user")
    {
        mark(context);
    }
    if messages.len() > 1
        && let Some(last) = messages.last_mut()
    {
        mark(last);
    }
    messages
}

fn read_call(call: &Value) -> Option<ToolCall> {
    let function = call.get("function")?;
    let raw = function["arguments"].as_str().unwrap_or("{}");
    Some(ToolCall {
        id: call.get("id")?.as_str()?.to_string(),
        name: function.get("name")?.as_str()?.to_string(),
        // Arguments that are not JSON stay as their text: the agent tells the model they were
        // incomplete instead of running the tool with none.
        arguments: serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string())),
    })
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;
