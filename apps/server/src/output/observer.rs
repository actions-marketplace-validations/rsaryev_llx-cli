//! [`Output`] as the agent's [`Observer`]: each call, its result, the model's turns, what it said.
//! The interface is told each as an event; so is the trace, which the interface's code draws.

use agent::observer::Observer;
use contracts::chat::{ToolCall, Usage};
use contracts::tool::{ToolError, ToolOutput};
use std::fmt::Write as _;

use super::{Output, human, json, logged};
use crate::printable;
use crate::server::label::{doing, label};
use crate::server::wire;
use crate::sync::lock;
use protocol::Event;

impl Observer for Output {
    fn call(&self, call: &ToolCall) {
        logged::call(&self.who(), call);
        self.recorded_call("", call);
        let label = label(&call.name, &call.arguments);
        let step = contracts::history::Step {
            what: call.name.clone(),
            detail: printable::cut(&doing(&label), 120),
            at_ms: self.millis(),
            ..Default::default()
        };
        lock(&self.done).push((call.id.clone(), step));
        let event = Event::Call {
            call: call.id.clone(),
            name: call.name.clone(),
            label,
        };
        if let Some(ui) = &self.ui {
            return ui.send(event);
        }
        if self.json {
            self.event(&json::call(call, self.millis()));
        }
        self.draw(&event);
    }

    fn result(&self, call: &ToolCall, outcome: &Result<ToolOutput, ToolError>) {
        logged::result(&self.who(), call, outcome);
        self.recorded_result("", call, outcome);
        let now = self.millis();
        let mut done = lock(&self.done);
        if let Some((_, step)) = done.iter_mut().rev().find(|(id, _)| *id == call.id) {
            step.ms = now.saturating_sub(step.at_ms);
            step.failed = outcome.is_err();
        }
        drop(done);
        let event = Event::Result {
            call: call.id.clone(),
            name: call.name.clone(),
            label: label(&call.name, &call.arguments),
            outcome: wire::outcome(outcome),
        };
        if let Some(ui) = &self.ui {
            return ui.send(event);
        }
        if self.json {
            self.event(&json::result(call, outcome, self.millis()));
        }
        self.draw(&event);
    }

    // `--json` has no event for it: the events are a contract, changed only with the schema.
    fn said(&self, text: &str) {
        logged::said(&self.who(), text);
        self.record("", serde_json::json!({"kind": "said", "text": text}));
        let said = Event::Answer(text.to_string());
        if let Some(ui) = &self.ui {
            return ui.send(said);
        }
        self.draw(&said);
    }

    fn fitted(&self, report: &agent::history::Report) {
        let note = self.fitted_by("", report);
        self.note(&note);
    }

    fn advised(&self, said: &str) {
        // The person reads the words; the model reads them in their tags.
        let words = said.replace("<review>", "").replace("</review>", "");
        log::info!("{}: advised: {words}", self.who());
        self.note(&format!("✻ {words}\n"));
    }

    fn turn(&self, usage: &Usage) {
        logged::turn(&self.who(), usage);
        if let Some(miss) = self.turn_by("", usage) {
            self.note(&miss);
        }
        // A lane's tokens: its agents' turns count in their own lane.
        if let Some(ui) = &self.ui {
            ui.send(Event::Spent(usage.input_tokens + usage.output_tokens));
        }
        if self.json {
            self.event(&json::turn(usage, self.millis()));
        }
        // With `-v`, in a session too: the line itself reaches the interface as a note.
        if self.verbose {
            let line = Event::Note(human::turn(usage));
            match &self.ui {
                Some(ui) => ui.send(line),
                None => self.draw(&line),
            }
        }
    }
}

impl Output {
    /// A model turn of `agent`'s (empty: the session's own), kept as a step of the task and
    /// counted in the session's spend; the note to show when it missed the prompt cache.
    pub(crate) fn turn_by(&self, agent: &str, usage: &Usage) -> Option<String> {
        let spent = contracts::history::Spent::from(usage);
        let at_ms = self.millis().saturating_sub(usage.millis);
        // The streams' clock is the wall's: a pause between tasks counts.
        let unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        let begun = unix_ms.saturating_sub(usage.millis);
        let miss = lock(&self.streams).request(agent, &usage.model, (begun, usage.millis), &spent);
        let mut detail = format!("{} in · {} out", usage.input_tokens, usage.output_tokens);
        if let Some(miss) = &miss {
            let _ = write!(detail, " · cache missed: {}", miss.cause.words());
        }
        let step = contracts::history::Step {
            what: "model".into(),
            detail,
            agent: agent.to_string(),
            model: usage.model.clone(),
            at_ms,
            ms: usage.millis,
            failed: false,
            spent,
        };
        lock(&self.done).push((String::new(), step));
        self.record(
            agent,
            serde_json::json!({
                "kind": "turn",
                "model": usage.model,
                "input_tokens": usage.input_tokens,
                "cached_tokens": usage.cached_tokens,
                "written_tokens": usage.written_tokens,
                "output_tokens": usage.output_tokens,
                "cost": usage.cost,
                "ms": usage.millis,
            }),
        );
        self.spend(spent);
        // A new conversation's first request is no news.
        miss.filter(|miss| miss.cause != crate::spend::Cause::Cold)
            .map(|miss| crate::spend::noted(agent, &miss))
    }

    /// `agent`'s context was elided before its next request, as `report` says: kept as a step,
    /// so the miss that follows is told by its cause. The note to show.
    pub(crate) fn fitted_by(&self, agent: &str, report: &agent::history::Report) -> String {
        let said: Vec<String> = report
            .iter()
            .map(|(stage, count)| format!("{count} {stage}"))
            .collect();
        let said = said.join(", ");
        lock(&self.streams).elided(agent);
        let step = contracts::history::Step {
            what: "elided".into(),
            detail: said.clone(),
            agent: agent.to_string(),
            at_ms: self.millis(),
            ..Default::default()
        };
        lock(&self.done).push((String::new(), step));
        self.record(agent, serde_json::json!({"kind": "elided", "stages": said}));
        let whose = match agent {
            "" => String::new(),
            agent => format!(" ({agent})"),
        };
        format!("✻ The context neared the model's limit{whose}; elided to save it: {said}\n")
    }

    /// A call of `agent`'s in the record, its arguments as the model wrote them.
    pub(crate) fn recorded_call(&self, agent: &str, call: &ToolCall) {
        self.record(
            agent,
            serde_json::json!({
                "kind": "call", "id": call.id, "name": call.name, "arguments": call.arguments,
            }),
        );
    }

    /// What a call of `agent`'s came to in the record, whole, as the model read it.
    pub(crate) fn recorded_result(
        &self,
        agent: &str,
        call: &ToolCall,
        outcome: &Result<ToolOutput, ToolError>,
    ) {
        let (content, failed) = match outcome {
            Ok(output) => (output.report.as_str(), false),
            Err(ToolError::Failed(why) | ToolError::Fatal(why)) => (why.as_str(), true),
        };
        self.record(
            agent,
            serde_json::json!({
                "kind": "result", "id": call.id, "name": call.name, "content": content,
                "failed": failed,
            }),
        );
    }
}
