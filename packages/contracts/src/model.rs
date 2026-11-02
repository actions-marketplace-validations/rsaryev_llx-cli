//! The decision-model port: typed questions about a state, answered with calibrated probabilities.
//! A decision model never writes text, so its output can only ever be one of the options it was
//! offered. Adapters implement [`Decider`] for a provider.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

/// One question: pick one key of `criteria`, or say how likely a statement is.
#[derive(Debug, Clone)]
pub enum Question {
    /// One key of `criteria` picked, each key's value saying what it stands for.
    Choice {
        instructions: Value,
        criteria: Map<String, Value>,
    },
    /// How likely the statement in `instructions` holds of the state.
    Noul { instructions: Value },
}

/// The answer to one question, of its kind.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// The key picked, and how likely each key was.
    Choice {
        choice: String,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
    /// The statement's probability, from 0 to 1.
    Noul(f64),
}

/// What came back for one state's questions.
#[derive(Debug, Clone, Default)]
pub struct Decision {
    /// Each answer under its question's key; a question the model left unanswered is not here.
    pub answers: BTreeMap<String, Answer>,
    /// The model that answered, as the provider names it.
    pub model: String,
    /// US dollars.
    pub cost: f64,
    pub millis: u64,
}

pub trait Decider: Send + Sync {
    fn decide(
        &self,
        state: &Value,
        questions: &BTreeMap<String, Question>,
    ) -> Result<Decision, DecideError>;
}

/// Why no decision came back.
#[derive(Debug, Clone, PartialEq)]
pub enum DecideError {
    /// The model could not be reached or refused the call (network, key, quota): asking again
    /// the same way will not help.
    Unavailable(String),
    /// It answered, but not with something usable.
    Invalid(String),
}

impl std::fmt::Display for DecideError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(message) | Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for DecideError {}
