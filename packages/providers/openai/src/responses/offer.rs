//! How a request offers the tools, and how their calls travel: a tool of free text (a patch) as
//! a `custom` tool held to its grammar where the model takes those, else as a function with one
//! string argument; every other tool as a function. The same choice decides how an earlier call
//! and its answer go back, and how a call comes in (ARCHITECTURE.md B5).

use std::collections::HashSet;

use contracts::chat::{Message, ToolCall};
use contracts::tool::{Input, ToolSpec};
use serde_json::{Value, json};

/// How the model is offered a tool of free text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Texts {
    /// As a function with one string argument: any model.
    Functions,
    /// As a `custom` tool, its input the raw text and held to its grammar: a model that takes
    /// those, as its catalog says.
    Custom,
}

/// A request's tools and what the conversation called of them.
pub(super) struct Offer<'a> {
    specs: &'a [ToolSpec],
    texts: Texts,
    /// The ids of the conversation's calls that went out as custom ones, whose answers do too.
    custom: HashSet<&'a str>,
}

impl<'a> Offer<'a> {
    pub(super) fn new(specs: &'a [ToolSpec], texts: Texts, conversation: &'a [Message]) -> Self {
        let mut offer = Self {
            specs,
            texts,
            custom: HashSet::new(),
        };
        let calls = conversation.iter().flat_map(|message| match message {
            Message::Assistant { calls, .. } => calls.as_slice(),
            Message::System(_)
            | Message::User(_)
            | Message::Image { .. }
            | Message::ToolResult { .. } => &[],
        });
        offer.custom = calls
            .filter(|call| offer.is_custom(&call.name))
            .map(|call| call.id.as_str())
            .collect();
        offer
    }

    fn spec(&self, name: &str) -> Option<&ToolSpec> {
        self.specs.iter().find(|spec| spec.name == name)
    }

    fn is_custom(&self, name: &str) -> bool {
        self.texts == Texts::Custom
            && self
                .spec(name)
                .is_some_and(|spec| matches!(spec.input, Input::Text(_)))
    }

    /// A tool as the request's `tools` list it.
    pub(super) fn tool(&self, spec: &ToolSpec) -> Value {
        match (&spec.input, self.texts) {
            (Input::Text(definition), Texts::Custom) => {
                json!({"type": "custom", "name": spec.name, "description": spec.description,
                    "format": {"type": "grammar", "syntax": "lark", "definition": definition}})
            }
            (Input::Text(_) | Input::Json(_), Texts::Functions | Texts::Custom) => {
                json!({"type": "function", "name": spec.name, "description": spec.description,
                    "parameters": spec.input.schema(), "strict": false})
            }
        }
    }

    /// An earlier call as an input item.
    pub(super) fn call_item(&self, call: &ToolCall) -> Value {
        if self.is_custom(&call.name) {
            let input = (call.arguments.as_str())
                .map_or_else(|| call.arguments.to_string(), str::to_string);
            return json!({"type": "custom_tool_call", "call_id": call.id, "name": call.name,
                "input": input});
        }
        let arguments = match self.spec(&call.name) {
            Some(spec) => spec.input.to_function(&call.arguments),
            None => call.arguments.clone(),
        };
        let arguments = arguments
            .as_str()
            .map_or_else(|| arguments.to_string(), str::to_string);
        json!({"type": "function_call", "call_id": call.id, "name": call.name,
            "arguments": arguments})
    }

    /// The answer to call `id` as an input item.
    pub(super) fn output_item(&self, id: &str, output: &str) -> Value {
        let kind = match self.custom.contains(id) {
            true => "custom_tool_call_output",
            false => "function_call_output",
        };
        json!({"type": kind, "call_id": id, "output": output})
    }

    /// A call the model made, as the agent takes it: a function call of a text tool has its text
    /// taken out of its one argument.
    pub(super) fn incoming(&self, mut call: ToolCall) -> ToolCall {
        if let Some(spec) = self.spec(&call.name) {
            call.arguments = spec.input.from_function(call.arguments);
        }
        call
    }
}

/// A `custom_tool_call` item as a call: its input is the text as the model wrote it.
pub(super) fn custom_call(item: &Value) -> Option<ToolCall> {
    Some(ToolCall {
        id: item["call_id"].as_str()?.to_string(),
        name: item["name"].as_str()?.to_string(),
        arguments: Value::String(item["input"].as_str().unwrap_or_default().to_string()),
    })
}

#[cfg(test)]
#[path = "offer_tests.rs"]
mod tests;
