//! What crosses to the interface, made from what the server knows: each domain value as its
//! wire form (`protocol`), in one place, so the interface never sees a contract, a tool or the
//! session store.

use contracts::tool::{ToolError, ToolOutput};

/// How a call ended, as the interface shows it: a refusal of the person's is not a failure.
pub(crate) fn outcome(outcome: &Result<ToolOutput, ToolError>) -> protocol::Outcome {
    match outcome {
        Ok(output) => protocol::Outcome::Done {
            report: output.report.clone(),
            shown: output.shown.clone(),
        },
        Err(ToolError::Failed(why)) => protocol::Outcome::Failed {
            declined: why.contains(crate::gate::DECLINED),
            why: why.clone(),
        },
        Err(ToolError::Fatal(why)) => protocol::Outcome::Fatal { why: why.clone() },
    }
}

/// A task of a session, as the transcript shows it again.
pub(crate) fn turn(turn: &contracts::history::Turn) -> protocol::Turn {
    protocol::Turn {
        goal: shown(&turn.goal),
        answer: turn.answer.clone(),
        status: turn.status.clone(),
    }
}

pub(crate) fn update(update: &crate::meta::Update) -> protocol::Update {
    protocol::Update {
        title: update.title.clone(),
    }
}

pub(crate) fn question(question: &contracts::person::Question) -> protocol::Question {
    protocol::Question {
        id: question.id.clone(),
        header: question.header.clone(),
        text: question.text.clone(),
        choices: (question.choices.iter())
            .map(|choice| protocol::Choice {
                label: choice.label.clone(),
                description: choice.description.clone(),
                preview: choice.preview.clone(),
            })
            .collect(),
        multiple: question.multiple,
    }
}

pub(crate) fn answer(answer: protocol::Answer) -> contracts::person::Answer {
    contracts::person::Answer {
        choices: answer.choices,
        note: answer.note,
    }
}

/// A task as the transcript shows it: what came back from the background reads as where it came
/// from (`arrivals::shown`), and `/init` as itself.
pub(crate) fn shown(goal: &str) -> String {
    // `/init` is sent as its instruction, and reads as what the person typed.
    if goal == crate::commands::extras::INIT {
        return "/init".into();
    }
    crate::arrivals::shown(goal).unwrap_or_else(|| goal.to_string())
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
