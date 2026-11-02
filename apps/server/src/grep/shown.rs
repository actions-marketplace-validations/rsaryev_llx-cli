//! What a search prints: each file found with its parts, line-numbered as an editor shows them,
//! then the files past `--limit` and those the model was unsure of, by name; on stderr, how the
//! files were picked and what it cost. `--files` prints the paths alone, `--json` one object.

use std::fmt::Write as _;
use std::time::Duration;

use serde_json::json;

use super::listing::Doc;
use super::{How, Outcome};
use crate::cli::GrepArgs;
use crate::error::{self, Failure};

/// A line longer than this is cut: a minified line says nothing more past it.
const LINE_WIDTH: usize = 240;

/// `outcome` printed as `args` asks, of the `read` files, in `took`.
pub(super) fn print(
    outcome: &Outcome,
    docs: &[Doc],
    args: &GrepArgs,
    read: usize,
    took: Duration,
) -> Result<(), Failure> {
    let path = |doc: usize| docs.get(doc).map_or("", |doc| doc.path.as_str());
    if args.json {
        return error::print(&format!("{}\n", as_json(outcome, docs, read, took)));
    }
    let mut out = String::new();
    if args.files {
        for found in &outcome.found {
            let _ = writeln!(out, "{}", path(found.doc));
        }
        error::print(&out)?;
        error::note(&summary(outcome, read, took));
        return Ok(());
    }
    let out = text(outcome, docs, args.limit);
    error::print(&out)?;
    error::note(&summary(outcome, read, took));
    Ok(())
}

/// Each of the first `limit` files found with its parts, line-numbered; then the rest and those
/// the model was unsure of, by name.
pub(super) fn text(outcome: &Outcome, docs: &[Doc], limit: usize) -> String {
    let path = |doc: usize| docs.get(doc).map_or("", |doc| doc.path.as_str());
    let mut out = String::new();
    for found in outcome.found.iter().take(limit) {
        let _ = writeln!(out, "{}  {:.2}", path(found.doc), found.score);
        let lines: Vec<&str> = docs
            .get(found.doc)
            .map_or(vec![], |doc| doc.text.lines().collect());
        let width = found
            .blocks
            .last()
            .map_or(1, |block| block.end.to_string().len());
        for (n, block) in found.blocks.iter().enumerate() {
            if n > 0 {
                let _ = writeln!(out, "{:>width$}", "..");
            }
            for (at, line) in lines.iter().enumerate().take(block.end).skip(block.start) {
                let line = super::judge::clipped(line, LINE_WIDTH);
                let _ = writeln!(out, "{:>width$}: {line}", at + 1);
            }
        }
        out.push('\n');
    }
    let more: Vec<String> = (outcome.found.iter().skip(limit))
        .map(|found| format!("{} ({:.2})", path(found.doc), found.score))
        .collect();
    if !more.is_empty() {
        let _ = writeln!(out, "also: {}", more.join(", "));
    }
    let maybe: Vec<String> = (outcome.maybe.iter())
        .map(|&(doc, yes)| format!("{} ({yes:.2})", path(doc)))
        .collect();
    if !maybe.is_empty() {
        let _ = writeln!(out, "unsure, read to check: {}", maybe.join(", "));
    }
    out
}

/// The line on stderr: how many files, how they were picked, the time and the cost.
pub(super) fn summary(outcome: &Outcome, read: usize, took: Duration) -> String {
    let found = outcome.found.len();
    let seconds = took.as_secs_f64();
    match &outcome.how {
        How::Judged {
            model,
            cost,
            judged,
        } => format!(
            "llx grep: {found} of {read} files answer, {judged} read by {model} ({seconds:.1} s, ${cost:.4})\n"
        ),
        How::Words(None) => {
            format!("llx grep: {found} of {read} files, by their words ({seconds:.1} s)\n")
        }
        How::Words(Some(why)) => format!(
            "llx grep: {found} of {read} files, by their words alone ({why}) ({seconds:.1} s)\n"
        ),
    }
}

fn as_json(outcome: &Outcome, docs: &[Doc], read: usize, took: Duration) -> serde_json::Value {
    let files: Vec<serde_json::Value> = (outcome.found.iter())
        .filter_map(|found| {
            let doc = docs.get(found.doc)?;
            let lines: Vec<&str> = doc.text.lines().collect();
            let blocks: Vec<serde_json::Value> = (found.blocks.iter())
                .map(|block| {
                    let text: Vec<&str> = lines
                        .iter()
                        .take(block.end)
                        .skip(block.start)
                        .copied()
                        .collect();
                    json!({"start": block.start + 1, "end": block.end, "text": text.join("\n")})
                })
                .collect();
            Some(json!({"file": doc.path, "score": found.score, "blocks": blocks}))
        })
        .collect();
    let maybe: Vec<serde_json::Value> = (outcome.maybe.iter())
        .filter_map(|&(doc, yes)| Some(json!({"file": docs.get(doc)?.path, "score": yes})))
        .collect();
    let (how, model, cost) = match &outcome.how {
        How::Judged { model, cost, .. } => ("judged", Some(model.as_str()), *cost),
        How::Words(_) => ("words", None, 0.0),
    };
    json!({
        "files": files, "unsure": maybe, "by": how, "model": model, "cost_usd": cost,
        "read": read, "millis": took.as_millis(),
    })
}
