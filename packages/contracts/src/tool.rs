//! The agent's tool protocol: a tool says what it is for and what arguments it takes, and turns a
//! call into text for the model. A tool knows nothing of the agent or the model that calls it.

use serde_json::{Value, json};

/// How a tool is offered to the model.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    /// What it does and how to use it well: all the model knows of the tool's world.
    pub description: String,
    /// What a call to it carries.
    pub input: Input,
}

/// What a tool takes. A call's `ToolCall::arguments` has the shape its input says: an object for
/// `Json`, a string (the text, as the model wrote it) for `Text`.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// Arguments, as a JSON Schema object: every tool but a text one.
    Json(Value),
    /// Free text (a patch), with nothing to escape, in the Lark grammar given. A provider that
    /// takes such tools offers it so, holding the model to the grammar; any other offers a
    /// function with one string argument, `source`.
    Text(String),
}

/// The one argument of a text tool offered as a function.
const TEXT_ARGUMENT: &str = "source";

impl Input {
    /// The JSON Schema of the function that stands for this input.
    pub fn schema(&self) -> Value {
        match self {
            Self::Json(schema) => schema.clone(),
            Self::Text(_) => json!({
                "type": "object",
                "properties": {TEXT_ARGUMENT: {"type": "string", "description": "The text, as it is."}},
                "required": [TEXT_ARGUMENT],
                "additionalProperties": false,
            }),
        }
    }

    /// The arguments of a function call as a call to the tool: a text tool's text taken out of
    /// its one argument. What is not that shape stays as it is, for the tool to refuse.
    #[must_use]
    pub fn from_function(&self, arguments: Value) -> Value {
        let text = match (self, &arguments) {
            (Self::Text(_), Value::Object(map)) if map.len() == 1 => map.get(TEXT_ARGUMENT),
            _ => None,
        };
        match text {
            Some(text @ Value::String(_)) => text.clone(),
            _ => arguments,
        }
    }

    /// A call's arguments as the function that stands for the tool takes them.
    pub fn to_function(&self, arguments: &Value) -> Value {
        match (self, arguments) {
            (Self::Text(_), Value::String(text)) => json!({TEXT_ARGUMENT: text}),
            _ => arguments.clone(),
        }
    }

    /// Whether `arguments` have the shape this input takes.
    pub fn accepts(&self, arguments: &Value) -> bool {
        match self {
            Self::Json(_) => arguments.is_object(),
            Self::Text(_) => arguments.is_string(),
        }
    }
}

/// What a call did: what the model reads, and what only the person sees.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolOutput {
    /// What happened: kept for the rest of the conversation.
    pub report: String,
    /// What only the person reads (an edit's diff): never sent to the model, and not kept.
    pub shown: String,
}

/// A report alone: most calls show the person nothing more.
impl From<String> for ToolOutput {
    fn from(report: String) -> Self {
        Self {
            report,
            ..Self::default()
        }
    }
}

/// An encoded picture, as a model reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    /// `image/jpeg`, `image/png`.
    pub media_type: String,
    /// The encoded bytes, in base64.
    pub data: String,
    /// In pixels.
    pub width: u32,
    pub height: u32,
}

/// Why a call did nothing useful.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolError {
    /// The call failed, but another call can do better (wrong arguments, a refused patch): the
    /// model reads why.
    Failed(String),
    /// Nothing will work now (what the tool depends on is unreachable): the run ends.
    Fatal(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(message) | Self::Fatal(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ToolError {}

pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    /// Do what `arguments` ask and say what happened.
    fn call(&self, arguments: &Value) -> Result<ToolOutput, ToolError>;
    /// Whether its calls may run at the same time as other such calls of one turn: they change
    /// nothing another call of the turn reads (reading, searching, commands). A call that is not
    /// runs alone, in its turn's order.
    fn parallel(&self) -> bool {
        false
    }
    /// A call the output limit cut off: what of it is whole carried out, and where it stopped
    /// said. `None` for a tool that cannot tell: its cut calls are never run, as a cut command
    /// may be missing its end (B7).
    fn salvage(&self, _cut: &Value) -> Option<Result<ToolOutput, ToolError>> {
        None
    }
}

#[cfg(test)]
#[path = "tool_tests.rs"]
mod tests;
