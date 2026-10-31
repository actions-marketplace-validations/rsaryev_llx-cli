//! The words of the trace that are not blocks of their own: a model turn for `-v`, what the run
//! spent, and the answer split between stdout and stderr.

use std::fmt::Write as _;

use contracts::chat::Usage;

use super::Summary;

/// One model turn, for `-v`.
pub(crate) fn turn(usage: &Usage) -> String {
    format!(
        "  · {} · {} in · {} out · ${:.4} · {:.1}s",
        usage.model,
        thousands(usage.input_tokens),
        thousands(usage.output_tokens),
        usage.cost,
        usage.millis as f64 / 1000.0
    )
}

/// Under the closing rule: the agent's model, tokens, money and time, and the session.
pub(crate) fn spend(summary: &Summary) -> String {
    let agent = summary.agent;
    let seconds = |ms: u64| ms as f64 / 1000.0;
    let mut out = String::new();
    if agent.calls > 0 {
        let _ = writeln!(
            out,
            "agent    {} · {} · {} out · ${:.4} · {:.1}s",
            agent.model,
            tokens_in(agent.input_tokens, agent.cached_tokens),
            thousands(agent.output_tokens),
            agent.cost,
            seconds(agent.millis)
        );
    }
    if !summary.session.is_empty() {
        let _ = writeln!(out, "session  {}", summary.session);
    }
    out
}

/// What goes to stdout and what to stderr at the end: the answer alone on stdout, or with
/// `--schema` the data alone on stdout (one line, for a pipeline) and the prose on stderr.
pub(crate) fn answer(summary: &Summary) -> (String, String) {
    let prose = summary.answer.trim();
    let line = |text: &str| match text.is_empty() {
        true => String::new(),
        false => format!("{text}\n"),
    };
    match summary.data {
        // A final message that is the data itself, fenced or not, is on stdout already.
        Some(Ok(data)) if said_as(prose).as_ref() == Some(data) => {
            (data.to_string(), String::new())
        }
        Some(Ok(data)) => (data.to_string(), line(prose)),
        Some(Err(why)) => (String::new(), format!("{}llx: {why}\n", line(prose))),
        None => (prose.to_string(), String::new()),
    }
}

/// `text` read as JSON, out of a Markdown fence if it is in one.
fn said_as(text: &str) -> Option<serde_json::Value> {
    let inner = text
        .strip_prefix("```")
        .map(|fenced| fenced.trim_start_matches("json").trim_end_matches("```"))
        .unwrap_or(text);
    serde_json::from_str(inner.trim()).ok()
}

/// `12,345 in (2,000 cached)`, or `12,345 in` when nothing was cached: `/usage` says it so too.
pub(crate) fn tokens_in(input: u64, cached: u64) -> String {
    match cached {
        0 => format!("{} in", thousands(input)),
        cached => format!("{} in ({} cached)", thousands(input), thousands(cached)),
    }
}

/// `12,345`
pub(crate) fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
#[path = "human_tests.rs"]
mod tests;
