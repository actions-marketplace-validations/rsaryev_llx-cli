//! The advisor port (C9): a decision model's word at a task's edges, where a few typed questions
//! cost less than a model turn. It never writes a call, nor changes the tools, the prompt or the
//! mode; with none, or one that fails or is late, the loop runs as it does without one.

use crate::catalog::Effort;
use crate::chat::Message;

/// A task as the advisor reads it.
#[derive(Debug, Clone, Copy)]
pub struct Task<'a> {
    /// The goal, as the person wrote it.
    pub goal: &'a str,
    /// The conversation so far.
    pub conversation: &'a [Message],
    /// Where this task's messages start in it.
    pub from: usize,
}

/// What the advisor says of the model's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Review {
    /// It ends the task, or nothing better is known.
    Done,
    /// The task is not done: the line the model reads before it goes on.
    GoOn(String),
}

/// What the loop asks at a task's edges; whoever composes the run answers.
pub trait Advisor: Send + Sync {
    /// The level a new conversation thinks at, asked as its first task (`goal`) starts, before
    /// its first request; `None`: the level chosen stands. A conversation that goes on (a
    /// follow-up, a fork) is not asked: its level stays, and so its cache. A resumed session opens
    /// a new one, with the earlier session in its context.
    fn level(&self, goal: &str) -> Option<Effort>;
    /// Once the model answers `task` with `answer`, a message with no call.
    fn review(&self, task: &Task, answer: &str) -> Review;
}
