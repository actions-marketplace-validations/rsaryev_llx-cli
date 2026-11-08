//! The chat-model port: a conversation and the tools on offer in, the model's next message out.
//! The conversation is typed here, not in any provider's wire format: an adapter maps it.

use serde_json::Value;

use crate::catalog::Effort;
use crate::tool::{Image, ToolSpec};

/// One message of a conversation.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// The standing instructions.
    System(String),
    User(String),
    /// What the model said: text, tool calls, or both.
    Assistant {
        text: Option<String>,
        calls: Vec<ToolCall>,
        /// What the provider asked to have sent back with this turn, as it gave it, opaque to
        /// everyone else: a model's reasoning, sealed, that it would otherwise lose between turns.
        carried: Vec<Value>,
    },
    /// What a tool call returned, for the call with `id`; `failed` when it did nothing useful.
    ToolResult {
        id: String,
        content: String,
        failed: bool,
    },
    /// A picture the person attached to their message. `caption`, sent beside it, says what it
    /// is (its name, its size).
    Image {
        caption: String,
        image: Image,
    },
}

impl Message {
    /// What the person said, if this is their message: a conversation's first words name it to a
    /// provider that keeps one's requests together (a session, a prompt cache's key).
    #[must_use]
    pub fn said(&self) -> Option<&str> {
        match self {
            Self::User(said) => Some(said),
            Self::System(_)
            | Self::Assistant { .. }
            | Self::Image { .. }
            | Self::ToolResult { .. } => None,
        }
    }
}

/// The model asking to use a tool.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// The arguments as the model wrote them: an object (the text itself for a text tool), or
    /// the text itself when they did not parse.
    pub arguments: Value,
}

/// The model's next message, and what it cost.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Completion {
    pub text: Option<String>,
    pub calls: Vec<ToolCall>,
    /// What goes back with this turn (`Message::Assistant::carried`).
    pub carried: Vec<Value>,
    /// The model's reasoning as readable text, when the host gives it so; a host reads `carried`,
    /// and may drop it once a user message follows.
    pub thought: Option<String>,
    pub usage: Usage,
    pub ending: Ending,
}

/// How a completion ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Ending {
    /// The model ended it.
    #[default]
    Whole,
    /// The model's output limit cut it off: its text, or a call's arguments, may stop midway.
    Cut,
}

impl Completion {
    /// The completion as a message to append to the conversation.
    pub fn message(&self) -> Message {
        Message::Assistant {
            text: self.text.clone(),
            calls: self.calls.clone(),
            carried: self.carried.clone(),
        }
    }
}

/// What one completion took.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Usage {
    /// The model that answered, as the provider names it.
    pub model: String,
    /// US dollars.
    pub cost: f64,
    /// How long the completion took.
    pub millis: u64,
    pub input_tokens: u64,
    /// Input tokens read from the provider's prompt cache: counted in `input_tokens` too.
    pub cached_tokens: u64,
    /// Input tokens written to the provider's prompt cache, where it bills them apart (at more
    /// than the input's price): counted in `input_tokens` too.
    pub written_tokens: u64,
    pub output_tokens: u64,
}

/// What a model spent over a run: its calls' usage, summed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Spend {
    /// The model that answered last, as its provider names it.
    pub model: String,
    pub calls: u64,
    pub cost: f64,
    pub millis: u64,
    pub input_tokens: u64,
    pub cached_tokens: u64,
    pub output_tokens: u64,
}

impl Spend {
    /// One more call's usage. An answer that names no model keeps the one before.
    pub fn add(&mut self, usage: &Usage) {
        if !usage.model.is_empty() {
            self.model.clone_from(&usage.model);
        }
        self.calls += 1;
        self.cost += usage.cost;
        self.millis += usage.millis;
        self.input_tokens += usage.input_tokens;
        self.cached_tokens += usage.cached_tokens;
        self.output_tokens += usage.output_tokens;
    }

    /// Add what another run spent (an agent this one started): its turns and tokens count here.
    pub fn absorb(&mut self, other: &Spend) {
        if self.model.is_empty() {
            self.model.clone_from(&other.model);
        }
        self.calls += other.calls;
        self.cost += other.cost;
        self.millis += other.millis;
        self.input_tokens += other.input_tokens;
        self.cached_tokens += other.cached_tokens;
        self.output_tokens += other.output_tokens;
    }
}

/// Why no completion came back.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatError {
    /// The model could not be reached or refused the call (network, key, quota).
    Unavailable(String),
    /// It answered, but not with something usable.
    Invalid(String),
}

impl std::fmt::Display for ChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(message) | Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ChatError {}

/// One request: the conversation so far and the tools the model may call.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub conversation: &'a [Message],
    /// Offered on every request, so the conversation's earlier calls stay valid.
    pub tools: &'a [ToolSpec],
    /// Which of them it may call now.
    pub calling: Calling<'a>,
    /// The latest moment the caller still wants an answer; `None`: the provider's own limit.
    pub deadline: Option<std::time::Instant>,
    /// The level this conversation thinks at, where it is not the model's chosen one (a decision
    /// model picked it as the conversation opened, under `auto`): taken where the model offers
    /// it, else the chosen level stands.
    pub effort: Option<Effort>,
    /// The task it is part of: a provider that routes a task's requests to where its cache is
    /// keeps them together by it.
    pub task: TaskId,
}

/// Which task a request is part of: the same for every request of one task, a new one for each
/// task, never given twice in a process (a rewind's task and the one it replaces are two).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TaskId(pub u64);

impl TaskId {
    /// A request of no task: a one-off (a check, a title).
    pub const NONE: Self = Self(0);
}

/// What a request lets the model call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Calling<'a> {
    /// Any of the tools, or none: the model decides.
    Any,
    /// None: the request asks for an answer.
    Nothing,
    /// The one tool named, and nothing else: the reply is that call.
    Only(&'a str),
}

pub trait Chat: Send + Sync {
    fn complete(&self, request: Request) -> Result<Completion, ChatError>;
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;
