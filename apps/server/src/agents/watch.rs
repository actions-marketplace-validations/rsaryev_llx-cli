//! What an agent's run shows, and how its approvals read. Its calls go to its own view in the
//! interface (the person switches to it) and, as one line each, under the call running in the
//! main session; its turns count in the trace and in `--json` as the task's. Its approvals name
//! it, so the person knows which agent asks.

use agent::observer::Observer;
use contracts::chat::{ToolCall, Usage};
use contracts::gate::{Effect, Gate, Verdict};
use contracts::tool::{ToolError, ToolOutput};

use crate::output::{Output, logged};
use crate::server::label::label;
use protocol::Event;

/// Watches one agent's run.
pub(crate) struct Watch<'a> {
    pub output: &'a Output,
    /// Its session id: where the interface shows it.
    pub id: &'a str,
    pub name: &'a str,
}

impl Watch<'_> {
    /// Send `event` to the agent's view.
    pub(crate) fn show(&self, event: Event) {
        if let Some(ui) = self.output.ui() {
            ui.for_lane(self.id).send(event);
        }
    }
}

impl Observer for Watch<'_> {
    fn call(&self, call: &ToolCall) {
        logged::call(self.id, call);
        let label = label(&call.name, &call.arguments);
        let what = format!("{} {}", label.done, label.subject)
            .trim()
            .to_string();
        self.show(Event::Call {
            call: call.id.clone(),
            name: call.name.clone(),
            label,
        });
        self.output.substep(format!("{}: {what}", self.name));
    }

    fn result(&self, call: &ToolCall, outcome: &Result<ToolOutput, ToolError>) {
        logged::result(self.id, call, outcome);
        self.show(Event::Result {
            call: call.id.clone(),
            name: call.name.clone(),
            label: label(&call.name, &call.arguments),
            outcome: crate::server::wire::outcome(outcome),
        });
    }

    fn said(&self, text: &str) {
        logged::said(self.id, text);
        self.show(Event::Answer(text.to_string()));
    }

    fn turn(&self, usage: &Usage) {
        self.show(Event::Spent(usage.input_tokens + usage.output_tokens));
        self.output.turn(usage);
    }
}

/// The gate as an agent asks it: each effect's summary starts with the agent's name.
pub(crate) struct Labeled<'a> {
    pub inner: &'a dyn Gate,
    pub label: String,
}

impl Gate for Labeled<'_> {
    fn check(&self, effect: Effect) -> Verdict {
        let summary = format!("[agent {}] {}", self.label, effect.summary);
        self.inner.check(Effect {
            summary: &summary,
            ..effect
        })
    }
}
