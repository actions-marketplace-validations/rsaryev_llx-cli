//! A tool call as a block: a bullet, what it does in a few words, and what it printed, its first
//! rows and its last with how many between were left out.
//!
//! ```text
//! • Ran cargo test --workspace
//!   │ --all-targets
//!   └ Compiling llx v0.1.0
//!     Finished `test` profile
//!     … +41 lines
//!     running 2 tests
//!     test parses ... ok
//!     test result: ok. 2 passed
//! ```
//!
//! The bullet is dim while the call runs (the live area draws a spinner there), green when it is
//! done and red when it failed. Everything a model or a tool wrote is escaped first, so it cannot
//! redraw the terminal.

use protocol::{Label, Outcome};
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::style;
use super::wrap::{cut, wrap};
use crate::printable::printable;

mod edit_diff;

/// Rows of output a finished call keeps: the first and the last, with how many were left out.
const HEAD: usize = 2;
const TAIL: usize = 3;

/// Where a call is.
pub(crate) enum State<'a> {
    Running,
    Done(&'a Outcome),
}

/// The block for a call of `name` that reads as `label`.
pub(crate) fn lines(name: &str, label: &Label, state: &State, width: usize) -> Vec<Line<'static>> {
    let mut out = head(name, label, state, width);
    if let State::Done(outcome) = state {
        out.extend(body(name, outcome, width));
    }
    out
}

/// A call the person said no to: not a failure, so not red; what they said is under it.
pub(crate) fn declined(label: &Label, said: &str, width: usize) -> Vec<Line<'static>> {
    let mut out = head("", label, &State::Running, width);
    out[0].spans[0] = Span::styled("• ", style::waiting());
    out[0].spans[1] = Span::styled("Skipped", style::bold());
    let text = match said.trim() {
        "" => "You said no".to_string(),
        said => format!("You said no: {}", printable(said)),
    };
    let line = Line::from(Span::styled(text, style::waiting()));
    out.extend(wrap(
        &line,
        width,
        &Line::from(Span::styled("  └ ", style::dim())),
        &Line::from("    "),
    ));
    out
}

/// The bullet and what the call does, wrapped under a rule.
pub(crate) fn head(name: &str, label: &Label, state: &State, width: usize) -> Vec<Line<'static>> {
    let (verb, bullet) = match state {
        State::Running => (&label.doing, style::dim()),
        State::Done(outcome) if failed(name, outcome) => (&label.done, style::failed()),
        State::Done(_) => (&label.done, style::done()),
    };
    let head = Line::from(vec![
        Span::styled(verb.clone(), style::bold()),
        Span::raw(" "),
        Span::raw(printable(&label.subject)),
    ]);
    // Wrapped as under `  ↳ `, the bullet a call takes when it joins the one before it
    // (`Screen::grouped`): two columns wider, it must not push the row past the screen.
    let mut rows = wrap(
        &head,
        width,
        &Line::from(Span::styled("    ", bullet)),
        &Line::from(Span::styled("  │ ", style::dim())),
    );
    if let Some(first) = rows.first_mut().and_then(|row| row.spans.first_mut()) {
        first.content = "• ".into();
    }
    rows
}

/// What a finished call printed or why it failed, under its head.
pub(crate) fn body(name: &str, outcome: &Outcome, width: usize) -> Vec<Line<'static>> {
    if name == "apply_patch"
        && let Outcome::Done { report, shown } = outcome
        && let Some(diffs) = shown.strip_prefix("Diff of ")
    {
        let diffs: Vec<&str> = diffs.split("\n\nDiff of ").collect();
        // One file changed: the head names it and the diff says how.
        let mut rows = output_rows(name, report);
        rows.retain(|_| diffs.len() > 1);
        let mut out = rows_block(rows, width);
        for each in &diffs {
            let (file, diff) = each.split_once(":\n").unwrap_or((each, ""));
            // Which file a diff is of, when there are several.
            if diffs.len() > 1 {
                let file = cut(&printable(file), width.saturating_sub(4).max(1));
                out.push(Line::from(Span::styled(
                    format!("    {file}"),
                    style::dim(),
                )));
            }
            out.extend(edit_diff::render(diff, width));
        }
        return out;
    }
    let rows = match outcome {
        Outcome::Failed { why, .. } | Outcome::Fatal { why } => {
            vec![(printable(why), style::failed())]
        }
        // Each question with its answer, as the person reads them, where the tool said so.
        // The person's own answers, whole: each wrapped under its question.
        Outcome::Done { shown, .. } if name == "ask" && !shown.is_empty() => {
            let answers = rows(shown.lines())
                .enumerate()
                .flat_map(|(at, (text, look))| {
                    let first = if at == 0 { "  └ " } else { "    " };
                    let first = Line::from(Span::styled(first, style::dim()));
                    wrap(
                        &Line::from(Span::styled(text, look)),
                        width,
                        &first,
                        &Line::from("    "),
                    )
                });
            return answers.collect();
        }
        Outcome::Done { report, .. } => output_rows(name, report),
    };
    rows_block(rows, width)
}

/// `rows` shortened, each cut to `width` under the call's rule.
fn rows_block(rows: Vec<(String, Style)>, width: usize) -> Vec<Line<'static>> {
    let mut out = vec![];
    for (at, (text, style)) in shorten(rows).into_iter().enumerate() {
        let prefix = if at == 0 { "  └ " } else { "    " };
        // An output row is cut, not wrapped: a long log line is noise past the first row.
        let text = cut(&text, width.saturating_sub(4).max(1));
        out.push(Line::from(vec![
            Span::styled(prefix, style::dim()),
            Span::styled(text, style),
        ]));
    }
    out
}

/// The chain a finished call is part of: the verb of one that succeeded, which a call of the same
/// verb printed right after it joins.
pub(super) fn chain(name: &str, label: &Label, outcome: &Outcome) -> Option<String> {
    (!failed(name, outcome)).then(|| label.done.clone())
}

fn failed(name: &str, outcome: &Outcome) -> bool {
    match outcome {
        Outcome::Failed { .. } | Outcome::Fatal { .. } => true,
        Outcome::Done { report, .. } => {
            name == "bash" && !exit_zero(report) && backgrounded(report).is_none()
        }
    }
}

/// What a command the background took says of itself: it was started there
/// (`run_in_background`) or moved there at its wait. It has not ended, so it neither failed nor
/// succeeded; its end comes later, as a task of its own.
fn backgrounded(report: &str) -> Option<String> {
    let head = report.lines().next()?;
    if head.starts_with("Running in the background:") {
        return Some("Running in the background".into());
    }
    let seconds = head
        .strip_prefix("Still running after: ")?
        .split_once(" seconds")?
        .0;
    Some(format!("Moved to the background after {seconds} s"))
}

/// What a call running does: `Running cargo test`.
pub(crate) fn doing(label: &Label) -> String {
    format!("{} {}", label.doing, label.subject)
        .trim()
        .to_string()
}

/// The rows worth showing under a finished call. A command shows what it printed, and its exit
/// when that was not 0; an edit the files it changed; the rest show their report's first row.
fn output_rows(name: &str, report: &str) -> Vec<(String, Style)> {
    match name {
        "bash" => bash_rows(report),
        // The files as the model reads them (`M src/a.rs`), without the line that says success.
        "apply_patch" => (report.lines().skip(1))
            .map(|line| (printable(line), style::dim()))
            .collect(),
        // What the person answered, without the report's own heading.
        "ask" => rows(report.lines()).skip(1).collect(),
        // What agents said, without the tags that frame it for the model.
        "agents" => rows(report.lines())
            .filter(|(row, _)| !row.starts_with("<agent_result") && row != "</agent_result>")
            .collect(),
        _ => rows(report.lines()).take(1).collect(),
    }
}

/// What a command printed: under it its exit when that was not 0, or how it went on in the
/// background.
fn bash_rows(report: &str) -> Vec<(String, Style)> {
    if let Some(said) = backgrounded(report) {
        let printed = report
            .lines()
            .skip_while(|line| *line != "Output so far:")
            .skip(1);
        let mut out: Vec<_> = rows(printed).collect();
        out.push((said, style::dim()));
        return out;
    }
    // The header's lines (`Exit code: 1`, `Wall time: …`) end at `Output:`.
    let head = report.lines().next().unwrap_or_default();
    let printed = report.lines().skip_while(|line| *line != "Output:").skip(1);
    let mut out: Vec<_> = rows(printed).collect();
    if !exit_zero(report) {
        out.push((printable(head), style::failed()));
    } else if out.is_empty() {
        out.push(("(no output)".into(), style::dim()));
    }
    out
}

/// The non-blank `lines`, each as a dim row.
fn rows<'a>(lines: impl Iterator<Item = &'a str>) -> impl Iterator<Item = (String, Style)> {
    lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| (printable(line.trim_end()), style::dim()))
}

/// Whether a command's report says it exited with 0.
fn exit_zero(report: &str) -> bool {
    report.lines().next() == Some("Exit code: 0")
}

/// At most `HEAD + TAIL + 1` rows: the first, `… +N lines`, the last.
fn shorten(rows: Vec<(String, Style)>) -> Vec<(String, Style)> {
    if rows.len() <= HEAD + TAIL + 1 {
        return rows;
    }
    let hidden = rows.len() - HEAD - TAIL;
    let mut out: Vec<(String, Style)> = rows[..HEAD].to_vec();
    out.push((format!("… +{hidden} lines"), style::dim()));
    out.extend_from_slice(&rows[rows.len() - TAIL..]);
    out
}

#[cfg(test)]
#[path = "tool_cell_tests.rs"]
mod tests;
