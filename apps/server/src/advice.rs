//! The advisor (C9) over the decision model: one decision as a conversation's first task starts
//! (the level it thinks at, under `auto`), one when the main session's model answers (whether the
//! task is done). Each is held to a few seconds (`providers::adviser`); a failure, a late answer or
//! a doubt changes nothing.

mod state;

use std::collections::BTreeMap;
use std::sync::Mutex;

use contracts::advice::{Advisor, Review, Task};
use contracts::catalog::Effort;
use contracts::model::{Answer, Decider, Decision, Question};
use serde_json::{Value, json};

use crate::choice::{Choice, Level};
use crate::sync::lock;

/// The odds the level chosen covers: the lowest whose odds and those below reach it.
const COVER: f64 = 0.8;
/// The odds the answer ends the task (done, or truly blocked) under which the task goes on.
const ENDS: f64 = 0.3;

const LEVEL: &str = "level";
const VERDICT: &str = "verdict";
const UNCHECKED: &str = "unchecked";

/// The levels a conversation may be given, lowest first (past them a turn is slow for little),
/// and what each is for, as the decision model reads it.
const LEVELS: [(Effort, &str); 4] = [
    (
        Effort::Low,
        "Routine work: a lookup, a small or mechanical change, a command to run.",
    ),
    (
        Effort::Medium,
        "Ordinary work: a feature or a fix across a few files, with checks to run.",
    ),
    (
        Effort::High,
        "Hard work: a subtle bug, a change across many files, careful reasoning.",
    ),
    (
        Effort::XHigh,
        "Very hard work: deep algorithmic or design reasoning, mistakes costly.",
    ),
];

/// The verdicts on an answer; the line the model reads for each that goes on.
const VERDICTS: [(&str, &str, Option<&str>); 5] = [
    (
        "done",
        "The answer meets the goal as the mode allows (where the mode is read-only, a plan of a \
         change meets a goal to change something), done and checked as the goal needs.",
        None,
    ),
    (
        "blocked",
        "The answer says exactly what blocks the goal, and it truly cannot be done from here (it \
         needs the person, an access or a fact that is not available).",
        None,
    ),
    (
        "unfinished",
        "Work the goal asks for and the mode allows is left: the answer lists steps still to do, \
         asks leave for what the goal already allows, or stops partway.",
        Some(
            "Your answer leaves part of the goal undone. Go on with what is left, and end only \
             when the goal is met or what remains truly cannot be done from here.",
        ),
    ),
    (
        UNCHECKED,
        "Files changed after the project's checks last ran, though the goal and the project need \
         them run.",
        Some(
            "Files changed since the project's checks last ran. Run the checks the change needs, \
             fix what fails, then end.",
        ),
    ),
    (
        "failing",
        "The last checks failed, or an error is left unresolved, and the answer neither fixes it \
         nor shows it is outside the goal.",
        Some(
            "The last checks failed or an error is left unresolved. Fix it, or say exactly why it \
             is outside the goal.",
        ),
    ),
];

/// What the advisors of a lane share: the decision model, where what it costs is counted (the
/// task's budget), and the levels a fresh conversation may be given while the level is `auto`.
pub(crate) struct Advice<'a> {
    decider: &'a dyn Decider,
    charge: &'a (dyn Fn(f64) + Sync),
    levels: Mutex<Vec<Effort>>,
}

/// Whom an advisor advises: the main session, or one of its agents (a level only: an agent's
/// answer goes to the main session, which checks it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Main,
    Agent,
}

/// One conversation's advisor.
pub(crate) struct ConversationAdvisor<'a> {
    advice: &'a Advice<'a>,
    role: Role,
}

impl<'a> Advice<'a> {
    pub(crate) fn new(decider: &'a dyn Decider, charge: &'a (dyn Fn(f64) + Sync)) -> Self {
        Self {
            decider,
            charge,
            levels: Mutex::new(vec![]),
        }
    }

    /// The levels of `choice` a fresh conversation may be given: under `auto`, those its model
    /// offers of [`LEVELS`]; none under a level the person chose.
    pub(crate) fn offer(&self, choice: &Choice) {
        let offered = choice.selection.model().offered();
        *lock(&self.levels) = match choice.level {
            Level::Auto => (LEVELS.iter())
                .map(|(effort, _)| *effort)
                .filter(|e| offered.contains(e))
                .collect(),
            Level::Fixed => vec![],
        };
    }

    pub(crate) fn advisor(&'a self, role: Role) -> ConversationAdvisor<'a> {
        ConversationAdvisor { advice: self, role }
    }
}

impl ConversationAdvisor<'_> {
    /// The decision, logged; `None` when none came in time.
    fn decide(
        &self,
        edge: &str,
        state: &Value,
        asked: &BTreeMap<String, Question>,
    ) -> Option<Decision> {
        match self.advice.decider.decide(state, asked) {
            Ok(decision) => {
                (self.advice.charge)(decision.cost);
                log::info!(
                    "advice as the task {edge}: {} in {} ms (${:.6}): {:?}",
                    decision.model,
                    decision.millis,
                    decision.cost,
                    decision.answers
                );
                Some(decision)
            }
            Err(error) => {
                log::info!("no advice as the task {edge}: {error}");
                None
            }
        }
    }
}

impl Advisor for ConversationAdvisor<'_> {
    fn level(&self, goal: &str) -> Option<Effort> {
        let levels = lock(&self.advice.levels).clone();
        if levels.len() < 2 {
            return None;
        }
        let criteria = (LEVELS.iter())
            .filter(|(effort, _)| levels.contains(effort))
            .map(|(effort, what)| (effort.name().to_string(), json!(what)))
            .collect();
        let instructions = json!(
            "How hard is this coding goal for an agent to reason about at each step? Pick the \
            effort it needs."
        );
        let asked = BTreeMap::from([(
            LEVEL.to_string(),
            Question::Choice {
                instructions,
                criteria,
            },
        )]);
        let state = json!({"goal": state::head(goal, state::TEXT)});
        let odds = odds(&self.decide("starts", &state, &asked)?, LEVEL)?;
        let mut covered = 0.0;
        levels.into_iter().find(|effort| {
            covered += odds.get(effort.name()).copied().unwrap_or(0.0);
            covered >= COVER
        })
    }

    fn review(&self, task: &Task, answer: &str) -> Review {
        if self.role == Role::Agent {
            return Review::Done;
        }
        let seen = state::review(task, answer);
        let criteria = (VERDICTS.iter())
            .filter(|(key, ..)| *key != UNCHECKED || seen.changed)
            .map(|(key, what, _)| ((*key).to_string(), json!(what)))
            .collect();
        let instructions = json!(
            "A coding agent ended its turn with this answer, working in the mode the state \
            names. Is the goal handled, as that mode allows?"
        );
        let asked = BTreeMap::from([(
            VERDICT.to_string(),
            Question::Choice {
                instructions,
                criteria,
            },
        )]);
        let Some(odds) = self
            .decide("ends", &seen.state, &asked)
            .and_then(|d| odds(&d, VERDICT))
        else {
            return Review::Done;
        };
        let p = |key: &str| odds.get(key).copied().unwrap_or(0.0);
        if p("done") + p("blocked") >= ENDS {
            return Review::Done;
        }
        let line = (VERDICTS.iter())
            .filter_map(|(key, _, line)| Some((p(key), (*line)?)))
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, line)| line);
        line.map_or(Review::Done, |line| {
            Review::GoOn(format!(
                "<review>A review of your answer, not the person's words: {line}</review>"
            ))
        })
    }
}

/// The odds of each option of the choice `key`.
fn odds(decision: &Decision, key: &str) -> Option<BTreeMap<String, f64>> {
    match decision.answers.get(key)? {
        Answer::Choice { probabilities, .. } => Some(probabilities.clone()),
        Answer::Noul(_) => None,
    }
}

#[cfg(test)]
#[path = "advice_tests.rs"]
mod tests;
