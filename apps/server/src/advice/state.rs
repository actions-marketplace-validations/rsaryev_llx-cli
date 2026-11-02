//! A task as the decision model reads it once the model answers (C9): the goal, each of the task's
//! calls with what it came to, the answer, and a few facts, cut to what that model reads. Tool
//! output stays data: it is a field of the state, never a question's words (C5).

use contracts::advice::Task;
use contracts::chat::{Message, ToolCall};
use serde_json::{Value, json};

/// Characters of the whole state at most: the decision model reads some 64k tokens.
pub(super) const MAX_STATE: usize = 60_000;
/// Characters kept of a call's input, of the end of its result, and of the goal and the answer.
const INPUT: usize = 400;
const OUTPUT: usize = 800;
pub(super) const TEXT: usize = 6_000;
/// The tool that changes files, and the one that runs the checks.
const PATCH: &str = "apply_patch";
const SHELL: &str = "bash";

/// What the decision model reads, and whether the task changed files.
pub(super) struct Seen {
    pub state: Value,
    pub changed: bool,
}

/// `task`, ended with `answer`.
pub(super) fn review(task: &Task, answer: &str) -> Seen {
    let own = task.conversation.get(task.from..).unwrap_or_default();
    let mut steps: Vec<(&ToolCall, Option<(&str, bool)>)> = vec![];
    // The steps in the order they came: each call, and what came in between (K4's arrivals, the
    // person's words), which the answer may report.
    let mut order: Vec<Shown<'_>> = vec![];
    let mut goal = true;
    for message in own {
        match message {
            Message::Assistant { calls, .. } => {
                order.extend((steps.len()..steps.len() + calls.len()).map(Shown::Call));
                steps.extend(calls.iter().map(|call| (call, None)));
            }
            Message::ToolResult {
                id,
                content,
                failed,
            } => {
                if let Some(step) = steps.iter_mut().rev().find(|(call, _)| call.id == *id) {
                    step.1 = Some((content.as_str(), *failed));
                }
            }
            // The task's own first message is its goal; a note on the mode is no step.
            Message::User(text) => {
                let note = text.starts_with("<mode_change>") || text.starts_with("<context>");
                if !std::mem::take(&mut goal) && !note {
                    order.push(Shown::Came(text));
                }
            }
            Message::System(_) | Message::Image { .. } => {}
        }
    }
    let worked = |(call, outcome): &(&ToolCall, Option<(&str, bool)>)| {
        call.name == PATCH && outcome.is_some_and(|(_, failed)| !failed)
    };
    let last_change = steps.iter().rposition(worked);
    let checked = last_change.is_some_and(|at| steps[at..].iter().any(|(c, _)| c.name == SHELL));
    let mut shown: Vec<Value> = (order.iter())
        .map(|shown| match shown {
            Shown::Call(at) => step(steps[*at].0, steps[*at].1),
            Shown::Came(text) => json!({"came_in": head(text, CAME)}),
        })
        .collect();
    let mut state = json!({
        "goal": head(task.goal, TEXT),
        "mode": mode(task.conversation).map(|mode| head(mode, INPUT * 4)),
        "answer": head(answer, TEXT),
        "facts": {
            "calls": steps.len(),
            "files_changed": last_change.is_some(),
            "checks_run_after_the_last_change": checked,
        },
    });
    // The latest steps, as many as the rest leaves room for.
    let mut room = MAX_STATE.saturating_sub(state.to_string().chars().count() + 100);
    let kept = (shown.iter().rev())
        .take_while(|step| {
            let size = step.to_string().chars().count() + 1;
            room.checked_sub(size).map(|left| room = left).is_some()
        })
        .count();
    state["facts"]["earlier_steps_left_out"] = json!(shown.len() - kept);
    state["steps"] = Value::Array(shown.split_off(shown.len() - kept));
    Seen {
        state,
        changed: last_change.is_some(),
    }
}

/// A step as the state lists it.
enum Shown<'a> {
    /// The call at this index of the steps.
    Call(usize),
    /// What came in between the calls.
    Came(&'a str),
}

/// What of a message that came in during the task the state keeps.
const CAME: usize = 1_500;

/// The mode the task works in, as the context or a later note states it: what is done in it
/// differs (in a read-only mode, a plan of a change is the answer).
fn mode(conversation: &[Message]) -> Option<&str> {
    conversation.iter().rev().find_map(|message| {
        let Message::User(text) = message else {
            return None;
        };
        if text.starts_with("<mode_change>") {
            line_after(text, "The mode is now ")
        } else if text.starts_with("<context>") {
            line_after(text, "\nMode: ")
        } else {
            None
        }
    })
}

/// The rest of the line `mark` starts in `text`.
fn line_after<'t>(text: &'t str, mark: &str) -> Option<&'t str> {
    let (_, rest) = text.split_once(mark)?;
    Some(rest.split_once('\n').map_or(rest, |(line, _)| line))
}

/// One call as the state shows it.
fn step(call: &ToolCall, outcome: Option<(&str, bool)>) -> Value {
    let input = match &call.arguments {
        // A patch reads as the files it names.
        Value::String(text) => (text.lines())
            .filter(|line| line.starts_with("*** ") && line.contains(':'))
            .collect::<Vec<_>>()
            .join("\n"),
        other @ (Value::Null
        | Value::Bool(_)
        | Value::Number(_)
        | Value::Array(_)
        | Value::Object(_)) => (other.get("command").and_then(Value::as_str))
            .map_or_else(|| other.to_string(), str::to_string),
    };
    let (output, failed) = outcome.unwrap_or(("(no result)", false));
    json!({"call": call.name, "input": head(&input, INPUT), "failed": failed,
        "output": tail(output, OUTPUT)})
}

/// The first `most` characters of `text`.
pub(super) fn head(text: &str, most: usize) -> &str {
    text.char_indices()
        .nth(most)
        .map_or(text, |(at, _)| text.get(..at).unwrap_or(text))
}

/// The last `most` characters of `text`.
fn tail(text: &str, most: usize) -> &str {
    let count = text.chars().count();
    let skip = count.saturating_sub(most);
    text.char_indices()
        .nth(skip)
        .map_or("", |(at, _)| text.get(at..).unwrap_or(text))
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
