//! The decision port over OpenRouter's `/systemone`: any of its decision models (the app names
//! which).

use std::collections::BTreeMap;

use contracts::model::{Answer, DecideError, Decider, Decision, Question};
use serde_json::{Map, Value, json};

use crate::client::{ApiError, OpenRouter};

pub struct SystemOne {
    pub client: OpenRouter,
    pub model: String,
    /// How long a decision is waited for, its retry included; `None`: the client's own limit.
    pub patience: Option<std::time::Duration>,
}

impl Decider for SystemOne {
    fn decide(
        &self,
        state: &Value,
        questions: &BTreeMap<String, Question>,
    ) -> Result<Decision, DecideError> {
        let questions: Map<String, Value> = questions
            .iter()
            .map(|(key, question)| {
                let body = match question {
                    Question::Choice {
                        instructions,
                        criteria,
                    } => json!({"type": "choice", "instructions": instructions,
                        "criteria": criteria}),
                    Question::Noul { instructions } => {
                        json!({"type": "noul", "instructions": instructions})
                    }
                };
                (key.clone(), body)
            })
            .collect();
        let body = json!({"model": self.model, "state": state, "questions": questions});
        let (value, millis) = self
            .client
            .post(
                "/systemone",
                &body,
                (self.patience).map(|patience| std::time::Instant::now() + patience),
            )
            .map_err(|error| match error {
                ApiError::Invalid(message) => DecideError::Invalid(message),
                other @ (ApiError::Auth(_) | ApiError::Http(..) | ApiError::Network(_)) => {
                    DecideError::Unavailable(format!("{} {other}", self.client.name))
                }
            })?;
        let mut decision = Decision {
            model: value
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or(&self.model)
                .to_string(),
            cost: value
                .pointer("/usage/cost")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            millis,
            ..Decision::default()
        };
        let answers = (value.get("answers").and_then(Value::as_object))
            .into_iter()
            .flatten();
        decision
            .answers
            .extend(answers.filter_map(|(key, answer)| Some((key.clone(), read_answer(answer)?))));
        Ok(decision)
    }
}

/// An answer as the `/systemone` wire carries it.
fn read_answer(answer: &Value) -> Option<Answer> {
    match answer.get("type")?.as_str()? {
        "choice" => Some(Answer::Choice {
            choice: answer.get("choice")?.as_str()?.to_string(),
            probabilities: answer
                .get("probabilities")?
                .as_object()?
                .iter()
                .filter_map(|(key, value)| Some((key.clone(), value.as_f64()?)))
                .collect(),
            confidence: answer
                .get("confidence")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        }),
        "noul" => Some(Answer::Noul(answer.get("noul")?.as_f64()?)),
        _ => None,
    }
}

#[cfg(test)]
#[path = "system_one_tests.rs"]
mod tests;
