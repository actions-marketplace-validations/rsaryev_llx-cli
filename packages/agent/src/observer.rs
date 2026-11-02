//! What a run shows as it goes: each tool call, what it came to, and what each model turn cost.
//! The agent reports and never waits on the answer, so a live trace, a progress line or a test
//! can watch a run without changing it.

use contracts::chat::{ToolCall, Usage};
use contracts::tool::{ToolError, ToolOutput};

/// Watches a run. Every event has a default that does nothing, so a watcher implements only the
/// ones it shows.
pub trait Observer {
    /// The model asked for `call`; it is about to be carried out.
    fn call(&self, _call: &ToolCall) {}
    /// What `call` came to, as the model reads it: every call gets one, including a `result` that
    /// breaks its schema, an unknown tool and a call that ends the run.
    fn result(&self, _call: &ToolCall, _outcome: &Result<ToolOutput, ToolError>) {}
    /// What the model said in a turn the run goes on from (beside its calls, in a reply the
    /// output limit cut off, or while it waits on what is still to come): not its answer, which
    /// the run ends with.
    fn said(&self, _text: &str) {}
    /// One model turn ended, and what it took.
    fn turn(&self, _usage: &Usage) {}
    /// The conversation neared the model's limit, and was prepared so: what each stage did.
    fn fitted(&self, _report: &crate::history::Report) {}
    /// The advisor's line, as the model reads it after its answer (C9).
    fn advised(&self, _said: &str) {}
}
