//! The decision-model port: typed questions about a state, answered with calibrated probabilities.
//! A decision model never writes text, so its output can only ever be one of the options it was
//! offered. Adapters implement [`Decider`] for a provider.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

/// One question: pick one key of `criteria`, or say how likely a statement is.
#[derive(Debug, Clone)]
pub enum Question {
    Choice {
        instructions: Value,
        criteria: Map<String, Value>,
    },
    Noul {
        instructions: Value,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Choice {
        choice: String,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
    Noul(f64),
}

#[derive(Debug, Clone, Default)]
pub struct Decision {
    pub answers: BTreeMap<String, Answer>,
    pub model: String,
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
