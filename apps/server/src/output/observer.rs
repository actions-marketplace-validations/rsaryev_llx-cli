//! [`Output`] as the agent's [`Observer`]: each call, its result, the model's turns, what it said.
//! The interface is told each as an event; so is the trace, which the interface's code draws.

use agent::observer::Observer;
use contracts::chat::{ToolCall, Usage};
use contracts::tool::{ToolError, ToolOutput};

use super::{Output, human, json, logged};
use crate::printable;
use crate::server::label::{doing, label};
use crate::server::wire;
use crate::sync::lock;
use protocol::Event;

impl Observer for Output {
    fn call(&self, call: &ToolCall) {
        logged::call(&self.who(), call);
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
        let said = Event::Answer(text.to_string());
        if let Some(ui) = &self.ui {
            return ui.send(said);
        }
        self.draw(&said);
    }

    fn fitted(&self, report: &agent::history::Report) {
        let said: Vec<String> = report
            .iter()
            .map(|(stage, count)| format!("{count} {stage}"))
            .collect();
        self.note(&format!(
            "✻ The context neared the model's limit; elided to save it: {}\n",
            said.join(", ")
        ));
    }

    fn advised(&self, said: &str) {
        // The person reads the words; the model reads them in their tags.
        let words = said.replace("<review>", "").replace("</review>", "");
        log::info!("{}: advised: {words}", self.who());
        self.note(&format!("✻ {words}\n"));
    }

    fn turn(&self, usage: &Usage) {
        logged::turn(&self.who(), usage);
        let spent = contracts::history::Spent::from(usage);
        let step = contracts::history::Step {
            what: "model".into(),
            detail: format!("{} in · {} out", usage.input_tokens, usage.output_tokens),
            at_ms: self.millis().saturating_sub(usage.millis),
            ms: usage.millis,
            failed: false,
            spent,
        };
        lock(&self.done).push((String::new(), step));
        self.spend(spent);
        // A lane's tokens: its agents' turns come here too.
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
