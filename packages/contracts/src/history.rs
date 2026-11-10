//! The history port: sessions of goals, kept by whatever store the app composes. A session is the
//! goals asked one after another with the same tools, and how each ended, so it can be listed and
//! continued. It has one writer at a time (ARCHITECTURE.md K8): the [`Record`] its store hands
//! out, which writes each change as it comes.

use serde::{Deserialize, Serialize};

use crate::chat::Usage;

#[derive(Debug, Clone, Default)]
pub struct Session {
    pub id: String,
    /// Unix seconds.
    pub created: u64,
    /// Unix seconds: when its last goal ended, or it was created.
    pub updated: u64,
    /// Where its goals ran, as the app names it (a directory): a session continued elsewhere is
    /// told apart from one continued in place.
    pub workspace: String,
    /// The session that started this one, when it is an agent another session's agent started.
    pub parent: Option<String>,
    /// An agent's short name, as its parent calls it; empty for a session the person started.
    pub name: String,
    /// What the session is about, as a title made from its first task; empty until one is.
    pub title: String,
    pub turns: Vec<Turn>,
}

/// One goal in a session, and how it ended.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub goal: String,
    pub answer: String,
    pub status: String,
    /// Unix seconds.
    pub at: u64,
    /// What the task did, in the order it began: each call and each model turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Step>,
}

/// One step of a task: a tool call, a model turn or an elision (`elided`), when it began and how
/// long it took.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// The tool's name, `model` for a model turn, `elided` for the context elided before one.
    pub what: String,
    /// What the call did, in a few words (`ls src`), or a turn's tokens.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// The agent whose step it is, by its short name: an agent's model turns are its parent's
    /// steps. Empty for the session's own.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub agent: String,
    /// The model that answered a model turn, as the provider names it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// Milliseconds from the task's start.
    pub at_ms: u64,
    pub ms: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub failed: bool,
    /// What it spent: a model turn's tokens and cost.
    #[serde(default, skip_serializing_if = "Spent::is_empty")]
    pub spent: Spent,
}

/// Tokens and money a step spent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Spent {
    pub input_tokens: u64,
    /// Input tokens read from the provider's prompt cache.
    pub cached_tokens: u64,
    /// Input tokens written to the provider's prompt cache, where it bills them apart.
    #[serde(default)]
    pub written_tokens: u64,
    pub output_tokens: u64,
    /// US dollars.
    pub cost: f64,
}

impl Spent {
    /// Whether it spent nothing, as a step that is not a model turn.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl From<&Usage> for Spent {
    fn from(usage: &Usage) -> Self {
        Self {
            input_tokens: usage.input_tokens,
            cached_tokens: usage.cached_tokens,
            written_tokens: usage.written_tokens,
            output_tokens: usage.output_tokens,
            cost: usage.cost,
        }
    }
}

impl Session {
    /// Record a finished goal; the session was updated when it ended.
    pub fn record(&mut self, turn: Turn) {
        self.updated = turn.at;
        self.turns.push(turn);
    }

    /// Whether another session started this one.
    pub fn is_agent(&self) -> bool {
        self.parent.is_some()
    }
}

/// Why a session was not saved or loaded; the causes are the store's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryError {
    /// No session is saved at all.
    Empty,
    /// No session has this id.
    NotFound(String),
    /// The session `id` is there but could not be read.
    Unreadable { id: String, why: String },
    /// The session `id` was read but is not a session.
    Corrupt { id: String, why: String },
    /// The session could not be written.
    Write(String),
    /// The session `id` has its writer already, in this process or another.
    Held(String),
}

impl std::fmt::Display for HistoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("no saved sessions"),
            Self::NotFound(id) => write!(f, "no session {id}"),
            Self::Unreadable { id, why } => write!(f, "session {id} cannot be read: {why}"),
            Self::Corrupt { id, why } => write!(f, "session {id} is not a session: {why}"),
            Self::Write(why) => f.write_str(why),
            Self::Held(id) => write!(
                f,
                "session {id} is open in another llx: quit it there first"
            ),
        }
    }
}

impl std::error::Error for HistoryError {}

/// Where sessions are kept.
pub trait History: Send + Sync {
    /// A new, empty session with a fresh id: nothing is written until it is held.
    fn start(&self) -> Session;
    /// The session with `id`.
    fn load(&self, id: &str) -> Result<Session, HistoryError>;
    /// Every session with a task, newest first.
    fn list(&self) -> Vec<Session>;
    /// `session`'s one writer, for as long as it lives (K8): a second, in this process or
    /// another, is refused with [`HistoryError::Held`]. A session it leaves with no task is not
    /// kept.
    fn hold(&self, session: &Session) -> Result<Box<dyn Record>, HistoryError>;
}

/// A held session's writer: each change written as it comes, never the session rewritten whole.
pub trait Record: Send + Sync {
    /// A task that ended.
    fn turn(&self, turn: &Turn) -> Result<(), HistoryError>;
    /// The session's title, made after its first task began.
    fn title(&self, title: &str) -> Result<(), HistoryError>;
}
