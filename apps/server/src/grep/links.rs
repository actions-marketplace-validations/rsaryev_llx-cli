//! The files a search reaches through the code's own links, after the decision model kept some:
//! the modules a kept file names (`tools::shell` of `tools::shell::query(..)`, an `import`'s or a
//! `require`'s), and the files not read yet that name the same ones, the rarer modules counting
//! more. A file that does what was asked through a module's function under a name the question
//! never says (`shell::query` for "starts a child process") has none of its words, so their
//! ranking never reaches it; its link to a file that answers does.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use contracts::model::Decider;

use super::judge::{self, Parts};
use super::listing::Doc;
use super::terms::Term;
use super::{JUDGED_LINES, MAYBE, asks_about_tests, bar, rank};

/// A module more than this share of the files name links nothing: `std::fmt`, `crate::error`.
const COMMON: f64 = 0.25;
/// At most this many files are reached through links.
pub(super) const MOST_LINKED: usize = 12;
/// The lines shown before and after each line that names a linked module.
const BEFORE: usize = 3;
const AFTER: usize = 8;

/// Names a module path never starts with: they say where it is, not what.
const SAY_NOTHING: [&str; 4] = ["crate", "self", "super", "Self"];

/// The modules `text` names: each `a::b::c` path's prefixes (`a::b`, `a`), and what an `import`,
/// a `from` or a `require` brings in.
pub(super) fn modules(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for line in text.lines() {
        paths(line, &mut found);
        imports(line, &mut found);
    }
    found
}

/// Each `a::b::c` path of `line`, as its prefixes but itself.
fn paths(line: &str, found: &mut BTreeSet<String>) {
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == ':';
    for token in line.split(|c: char| !word(c)) {
        if !token.contains("::") {
            continue;
        }
        let segments: Vec<&str> = token.split("::").collect();
        // `use a::b::{c, d}` ends with `::`: then `a::b` is the module and named whole.
        let whole = segments.last().is_some_and(|last| last.is_empty());
        let named: Vec<&str> = segments.into_iter().filter(|s| !s.is_empty()).collect();
        let upto = if whole { named.len() } else { named.len() - 1 };
        let start = named
            .iter()
            .position(|segment| !SAY_NOTHING.contains(segment))
            .unwrap_or(named.len());
        for end in (start + 1)..=upto {
            if let Some(prefix) = named.get(start..end) {
                found.insert(prefix.join("::"));
            }
        }
    }
}

/// What `line` imports: `import x`, `from x import y`, `from "x"`, `require("x")`. Prose says
/// "from" too: a quote, or `import` after it, tells code from it.
fn imports(line: &str, found: &mut BTreeSet<String>) {
    let name = |rest: &str| -> Option<String> {
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || "_./@-:".contains(*c))
            .collect();
        (name.len() > 1).then_some(name)
    };
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("import ") {
        found.extend(name(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("from ")
        && line.contains(" import ")
    {
        found.extend(name(rest));
    }
    for keyword in [" from ", "require("] {
        if let Some((_, rest)) = line.split_once(keyword)
            && let Some(rest) = rest.strip_prefix(['"', '\''])
        {
            found.extend(name(rest));
        }
    }
}

/// The files not `settled` (and `kept`) that name the modules the `kept_files` name, best linked
/// first, each with the modules it shares, rarest first.
pub(super) fn linked(
    docs: &[Doc],
    kept_files: &[usize],
    settled: &[bool],
    kept: impl Fn(usize) -> bool,
) -> Vec<(usize, Vec<String>)> {
    let named: Vec<BTreeSet<String>> = docs.iter().map(|doc| modules(&doc.text)).collect();
    // Counted as floats: a count of files is far below 2^52, and no cast is needed.
    let mut files_naming: BTreeMap<&str, f64> = BTreeMap::new();
    for module in named.iter().flatten() {
        *files_naming.entry(module).or_default() += 1.0;
    }
    let all: f64 = docs.iter().map(|_| 1.0).sum();
    let weight = |module: &str| -> Option<f64> {
        let count = *files_naming.get(module)?;
        (count >= 2.0 && count / all <= COMMON).then(|| (all / count).ln())
    };
    let wanted: BTreeMap<&str, f64> = (kept_files.iter())
        .filter_map(|&doc| named.get(doc))
        .flatten()
        .filter_map(|module| Some((module.as_str(), weight(module)?)))
        .collect();
    let mut reached: Vec<(usize, f64, Vec<String>)> = (0..docs.len())
        .filter(|&doc| !settled.get(doc).copied().unwrap_or(true) && kept(doc))
        .filter_map(|doc| {
            let mut shared: Vec<(&str, f64)> = (named.get(doc)?.iter())
                .filter_map(|module| Some((module.as_str(), *wanted.get(module.as_str())?)))
                .collect();
            // The rarest first, and of those as rare, the most precise: `tools::shell`, not `tools`.
            shared.sort_by(|a, b| b.1.total_cmp(&a.1).then(b.0.len().cmp(&a.0.len())));
            let score: f64 = shared.iter().map(|(_, weight)| weight).sum();
            let shared = shared.into_iter().map(|(m, _)| m.to_string()).collect();
            (score > 0.0).then_some((doc, score, shared))
        })
        .collect();
    reached.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    reached.truncate(MOST_LINKED);
    reached
        .into_iter()
        .map(|(doc, _, shared)| (doc, shared))
        .collect()
}

/// The lines of `text` around those that name one of `modules`, in at most `lines` lines, each
/// part headed with its lines as the model reads them.
pub(super) fn around(text: &str, modules: &[String], lines: usize) -> Parts {
    let rows: Vec<&str> = text.lines().collect();
    let names = |row: &str| {
        modules.iter().any(|module| {
            row.contains(&format!("{module}::"))
                || row.contains(&format!("{module}\""))
                || row.contains(&format!("{module}'"))
                || row.split_whitespace().any(|word| word == module)
        })
    };
    let mut spans: Vec<(usize, usize)> = vec![];
    for (at, row) in rows.iter().enumerate() {
        if !names(row) {
            continue;
        }
        let (start, end) = (at.saturating_sub(BEFORE), (at + AFTER + 1).min(rows.len()));
        match spans.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => spans.push((start, end)),
        }
    }
    let mut parts = Parts {
        text: String::new(),
        spans: vec![],
        via: modules.to_vec(),
    };
    let mut left = lines;
    for (start, end) in spans {
        let end = end.min(start + left);
        if end <= start {
            break;
        }
        let n = parts.spans.len() + 1;
        let _ = writeln!(parts.text, "[part {n}: lines {}-{end}]", start + 1);
        for row in rows.iter().take(end).skip(start) {
            parts.text.push_str(row);
            parts.text.push('\n');
        }
        parts.spans.push((start, end));
        left -= end - start;
    }
    parts
}

/// The files the model kept, with those they link to read again: a second round, only when
/// some answer. The files reached through the modules a kept one names are read where they name
/// them, those not read yet and those the model was unsure of (it read them by the question's
/// words, which they may not have); a second reading replaces the first. One that fails leaves
/// the first round as it is.
pub(super) fn second_round(
    decider: &dyn Decider,
    query: &str,
    docs: &[Doc],
    terms: &[Term],
    mut candidates: Vec<usize>,
    mut verdicts: judge::Verdicts,
) -> (Vec<usize>, judge::Verdicts, Vec<bool>) {
    let mut second = vec![false; candidates.len()];
    let judged: Vec<(usize, Option<f64>)> = (candidates.iter().copied())
        .zip(verdicts.yes.iter().copied())
        .collect();
    // The files shown link; one the margin left out, and the doubted, are read again.
    let best = (judged.iter())
        .filter_map(|(_, yes)| *yes)
        .fold(0.0, f64::max);
    let shown = bar(best);
    let kept_files: Vec<usize> = (judged.iter())
        .filter(|(_, yes)| yes.is_some_and(|yes| yes >= shown))
        .map(|(doc, _)| *doc)
        .collect();
    if kept_files.is_empty() {
        return (candidates, verdicts, second);
    }
    // Settled: read, and answered either way with some certainty.
    let mut settled = vec![false; docs.len()];
    for (doc, yes) in &judged {
        settled[*doc] = !yes.is_some_and(|yes| (MAYBE..shown).contains(&yes));
    }
    let tests_asked = asks_about_tests(terms);
    let kept = |doc: usize| tests_asked || docs.get(doc).is_some_and(|d| !rank::is_test(&d.path));
    let reached = linked(docs, &kept_files, &settled, kept);
    let parts: Vec<(&Doc, Parts)> = (reached.iter())
        .filter_map(|(doc, modules)| {
            let doc = docs.get(*doc)?;
            Some((doc, around(&doc.text, modules, JUDGED_LINES)))
        })
        .collect();
    if parts.is_empty() {
        return (candidates, verdicts, second);
    }
    let Ok(again) = judge::judged(decider, query, &parts) else {
        return (candidates, verdicts, second);
    };
    for (((doc, _), yes), lead) in reached.iter().zip(again.yes).zip(again.lead) {
        match candidates.iter().position(|read| read == doc) {
            Some(at) => {
                let slots = (verdicts.yes.get_mut(at), verdicts.lead.get_mut(at));
                if let (Some(slot), Some(lead_slot), Some(yes)) = (slots.0, slots.1, yes) {
                    *slot = Some(yes);
                    *lead_slot = lead;
                    second[at] = true;
                }
            }
            None => {
                candidates.push(*doc);
                verdicts.yes.push(yes);
                verdicts.lead.push(lead);
                second.push(true);
            }
        }
    }
    verdicts.cost += again.cost;
    (candidates, verdicts, second)
}

#[cfg(test)]
#[path = "links_tests.rs"]
mod tests;
