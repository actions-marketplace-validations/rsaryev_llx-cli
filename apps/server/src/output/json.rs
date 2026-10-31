//! `--json`: the run as JSON lines, one event each, in the order they happen. `ms` (on a
//! call, its result and a turn) is the time since the task started. Every event has a `type`:
//! - `run.started`: `schema`, `goal`, `session`, `model`, `workspace`
//! - `tool.call`: `id`, `name`, `arguments`
//! - `tool.result`: `id`, `name`, `failed`, `fatal`, `output`
//! - `turn`: one model turn: `model`, tokens, `cost_usd`
//! - `run.completed`: `status`, `exit_code`, `answer`, `data` or `data_error`, `error`,
//!   `session`, `usage` {`agent`, `cost_usd`}, `duration_ms`

use contracts::chat::Spend;
use contracts::chat::{ToolCall, Usage};
use contracts::tool::{ToolError, ToolOutput};
use serde_json::{Value, json};

use super::{SCHEMA, Start, Summary};

pub(super) fn started(start: &Start) -> Value {
    json!({
        "type": "run.started",
        "schema": SCHEMA,
        "goal": start.goal,
        "session": start.session,
        "model": start.model,
        "workspace": start.workspace,
    })
}

pub(super) fn call(call: &ToolCall, ms: u64) -> Value {
    json!({
        "type": "tool.call",
        "id": call.id,
        "name": call.name,
        "arguments": call.arguments,
        "ms": ms,
    })
}

pub(super) fn result(call: &ToolCall, outcome: &Result<ToolOutput, ToolError>, ms: u64) -> Value {
    let mut event = json!({
        "type": "tool.result",
        "id": call.id,
        "name": call.name,
        "failed": outcome.is_err(),
        "fatal": matches!(outcome, Err(ToolError::Fatal(_))),
        "ms": ms,
    });
    match outcome {
        Ok(output) => event["output"] = json!(output.report),
        Err(error) => event["output"] = json!(error.to_string()),
    }
    event
}

pub(super) fn turn(usage: &Usage, ms: u64) -> Value {
    json!({
        "type": "turn",
        "model": usage.model,
        "input_tokens": usage.input_tokens,
        "cached_tokens": usage.cached_tokens,
        "output_tokens": usage.output_tokens,
        "cost_usd": usage.cost,
        "model_ms": usage.millis,
        "ms": ms,
    })
}

pub(super) fn completed(summary: &Summary) -> Value {
    let mut event = json!({
        "type": "run.completed",
        "status": summary.status,
        "exit_code": summary.code,
        "answer": summary.answer,
        "error": summary.error,
        "session": summary.session,
        "usage": {
            "agent": agent(summary.agent),
            "cost_usd": summary.agent.cost,
        },
        "duration_ms": summary.millis,
    });
    match summary.data {
        Some(Ok(data)) => event["data"] = data.clone(),
        Some(Err(why)) => event["data_error"] = json!(why),
        None => {}
    }
    event
}

fn agent(spend: &Spend) -> Value {
    json!({
        "model": spend.model,
        "calls": spend.calls,
        "input_tokens": spend.input_tokens,
        "cached_tokens": spend.cached_tokens,
        "output_tokens": spend.output_tokens,
        "cost_usd": spend.cost,
        "ms": spend.millis,
    })
}

#[cfg(test)]
#[path = "json_tests.rs"]
mod tests;
