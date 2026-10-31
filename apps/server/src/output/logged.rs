//! A run as the log keeps it (`logs.rs`), whoever shows it: each task's start and end, each call
//! with its arguments as the model wrote them and how it ended, the model's turns and words. A
//! line names who ran it (the session, or the agent), so a session's lines can be read alone.

use contracts::chat::{ToolCall, Usage};
use contracts::tool::{ToolError, ToolOutput};
use log::{debug, error, info, warn};

use super::{Start, Summary};

const TARGET: &str = "llx::session";

pub(super) fn started(start: &Start) {
    info!(
        target: TARGET,
        "{} task started: model={} workspace={} goal={:?}",
        start.session, start.model, start.workspace, start.goal
    );
}

pub(crate) fn call(who: &str, call: &ToolCall) {
    info!(target: TARGET, "{who} call {} {} {}", call.id, call.name, call.arguments);
}

pub(crate) fn result(who: &str, call: &ToolCall, outcome: &Result<ToolOutput, ToolError>) {
    match outcome {
        Ok(output) => debug!(
            target: TARGET,
            "{who} done {} {}: {} bytes",
            call.id,
            call.name,
            output.report.len()
        ),
        Err(ToolError::Failed(why)) => {
            warn!(target: TARGET, "{who} failed {} {}: {why}", call.id, call.name);
        }
        Err(ToolError::Fatal(why)) => {
            error!(target: TARGET, "{who} fatal {} {}: {why}", call.id, call.name);
        }
    }
}

pub(crate) fn said(who: &str, text: &str) {
    debug!(target: TARGET, "{who} said: {text}");
}

pub(crate) fn turn(who: &str, usage: &Usage) {
    debug!(
        target: TARGET,
        "{who} turn: {} in ({} cached), {} out, {} ms",
        usage.input_tokens, usage.cached_tokens, usage.output_tokens, usage.millis
    );
}

pub(super) fn completed(summary: &Summary) {
    let spent = summary.agent.cost;
    info!(
        target: TARGET,
        "{} task ended: {} (exit {}) in {} ms, {} turns, ${spent:.4}",
        summary.session, summary.status, summary.code, summary.millis, summary.agent.calls
    );
    debug!(target: TARGET, "{} answer: {}", summary.session, summary.answer);
    if let Some(why) = summary.error {
        warn!(target: TARGET, "{} task error: {why}", summary.session);
    }
}
