//! The contracts every part of llx speaks, and nothing else: no logic, no technology.
//! - [`advice`]: the advisor port: a decision model's word at a task's edges (C9).
//! - [`chat`]: the chat-model port: a conversation in, the next message out. A provider package
//!   implements [`chat::Chat`].
//! - [`catalog`]: what a provider offers: models and the reasoning levels each takes, and a
//!   [`catalog::Selection`] of one, which holds only a level its model offers.
//! - [`tool`]: the agent's tools: a spec the model reads and a call that returns text. A tool
//!   package implements [`tool::Tool`].
//! - [`model`]: the decision-model port: typed questions answered with probabilities. A provider
//!   package implements [`model::Decider`].
//! - [`gate`]: a tool asks before an effect (a command, a file change); the app answers.
//! - [`person`]: a tool asks the person a question; the app answers, or says nobody can.
//! - [`history`]: the session-history port. A store package implements [`history::History`].
//! - [`inbox`]: what reaches a run between its turns (the person's words, background work done).
//! - [`shape`]: the shape a run's result must have (`--schema`, ARCHITECTURE.md P8): the schema
//!   the model is told, and every way data does not match it.

pub mod advice;
pub mod catalog;
pub mod chat;
pub mod gate;
pub mod history;
pub mod inbox;
pub mod model;
pub mod person;
pub mod shape;
pub mod tool;
