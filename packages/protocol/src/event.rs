//! What the server tells the interface happened, each in a session (a lane or an agent) or in
//! the server as a whole. Events are data the interface shows; none waits for an answer.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Event {
    /// The session's first row: the version and what it runs with.
    Header(Vec<String>),
    /// A quiet line; its rows are kept as they are.
    Note(String),
    Error(String),
    /// The mode changed: its name, `default` included.
    Mode(String),
    /// A task began, as the transcript shows it (`≋ ticks · tick 1` for a monitor's lines).
    TaskStarted(String),
    /// A call began: `call` is its id, the same on its result.
    Call {
        call: String,
        name: String,
        label: Label,
    },
    /// The call `call` ended, and how.
    Result {
        call: String,
        name: String,
        label: Label,
        outcome: Outcome,
    },
    /// One step under the call running (what an agent it started does). Live only: never
    /// replayed (W6).
    Step(String),
    /// The model's answer.
    Answer(String),
    /// The task ended: how (`status`), in how long, in how many model turns, at what cost (US
    /// dollars).
    TaskEnded {
        status: String,
        millis: u64,
        turns: u64,
        cost: f64,
    },
    /// A resumed session's tasks so far, oldest first.
    Earlier(Vec<Turn>),
    /// What is known of the session changed.
    Updated(Update),
    /// A session of the interface began.
    LaneStarted {
        id: String,
        name: String,
    },
    /// The session, a lane, ended: the interface lets it go, with its agents, and nothing more
    /// is said in it.
    LaneEnded,
    /// This directory's saved sessions, newest first.
    Saved(Vec<Saved>),
    /// Show the session: it was opened, or asked for and open already.
    Show(String),
    /// The session started an agent.
    Started {
        id: String,
        name: String,
    },
    /// The agent was closed: it stays in the tree, dimmed.
    Closed,
    /// Tokens a model turn of the session (a lane or an agent) used, in and out.
    Spent(u64),
    /// What the session spent in all so far, its agents included: sent whole,
    /// never as a change, so a client that missed one is right at the next.
    Total(Total),
    /// The lane's conversation went back or began again: its agents are gone.
    AgentsCleared,
    /// Words put in the composer for the person to change and send (a rewound task).
    Prefill(String),
    /// The share of the model's context window left, in percent, after a task.
    Context(u8),
    /// The session's jobs, all of them, whenever they change.
    Jobs(Vec<JobRow>),
    /// What waits in the lane: the person's words for the task running, their entries for after
    /// it, then what came back from the background (as the transcript will show it); `ready`
    /// when the lane waits for its next entry.
    Queue {
        ready: bool,
        steered: Vec<String>,
        queued: Vec<String>,
        background: Vec<String>,
    },
    /// The task running read what the person said to it: shown now as theirs.
    Steered(Vec<String>),
    /// Text for the client's clipboard (`/copy`): the clipboard is the person's, not the server's.
    Copy(String),
    /// The interface's work is over: the terminal is given back.
    Quit,
}

/// What a call does, in a few words, as the server that knows the tool says it: the verb while
/// it runs and once done, and its subject (`Running`/`Ran` `cargo test`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub doing: String,
    pub done: String,
    pub subject: String,
}

/// Tokens and money, in all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Total {
    pub input_tokens: u64,
    /// Input tokens read from the provider's prompt cache.
    pub cached_tokens: u64,
    pub output_tokens: u64,
    /// US dollars.
    pub cost: f64,
}

/// How a call ended, as the interface shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Outcome {
    Done {
        report: String,
        /// What only the person reads under the call (an edit's diff). Optional (W3): an older
        /// server sends none.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        shown: String,
    },
    /// The model is told why; `declined` when the person refused the call.
    Failed { why: String, declined: bool },
    /// The run ends on it.
    Fatal { why: String },
}

/// A task of a session as the transcript shows it again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    /// What was asked, as shown.
    pub goal: String,
    pub answer: String,
    pub status: String,
}

/// A change to a session's facts; a field left `None` stays as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Update {
    pub title: Option<String>,
}

/// A saved session, as the list shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub id: String,
    /// Its title, else its first task.
    pub title: String,
    /// When it ran and how many tasks it has (`2 h ago · 4 tasks`).
    pub meta: String,
}

/// A job as the list under the composer shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRow {
    pub id: String,
    pub name: String,
    /// Its command, or its cron and prompt.
    pub detail: String,
    /// A monitor's deadline, milliseconds since the epoch.
    pub until_at: Option<u64>,
    /// A schedule's next run, as a clock shows it (`14:05`).
    pub next: Option<String>,
    /// Batches a monitor gave.
    pub events: usize,
}
