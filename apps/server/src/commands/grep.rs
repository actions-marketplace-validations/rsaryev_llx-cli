//! `llx grep`: a search of the code by what a question means, for a person or an agent. The files
//! are ranked by the question's words first (BM25, with no model); where a decision model is
//! there it then reads the best of them and keeps those that answer the question, so a file that
//! does the thing under another name is found and one that only mentions it is left out. Without
//! one, the ranking by words is the answer. Each file is shown with the parts that matter, whole
//! functions or sections where they are short.

use std::fmt::Write as _;
use std::time::Instant;

use crate::cli::GrepArgs;
use crate::error::Failure;
use crate::exit;
use crate::providers;
use crate::sync::each_at_once;

mod blocks;
mod judge;
mod links;
mod listing;
mod rank;
mod shown;
mod terms;

use blocks::Block;
use listing::Doc;
use terms::Term;

/// A file judged at least this likely to answer, and no more than [`BELOW_BEST`] below the most
/// likely one, is shown; below that, down to [`MAYBE`], named. The model leans to yes on a file
/// near the subject: a file far below the best is seldom what was asked.
const YES: f64 = 0.5;
const BELOW_BEST: f64 = 0.25;
const MAYBE: f64 = 0.2;
/// At most this many files the model was unsure of are named.
const MOST_UNSURE: usize = 8;
/// The lines the model reads of a long file's parts.
const JUDGED_LINES: usize = 60;
/// How many files are read from disk at once.
const READERS: usize = 8;

/// The lines of the part the decision model said answers best, `start..end`.
type Lead = Option<(usize, usize)>;

/// A file the search shows: its index among those read, how well it answers, its parts, and the
/// part the decision model said answers best.
pub(super) struct Found {
    doc: usize,
    score: f64,
    blocks: Vec<Block>,
    lead: Lead,
}

/// How the files were picked.
enum How {
    /// The decision model read `judged` files.
    Judged {
        model: String,
        cost: f64,
        judged: usize,
    },
    /// By their words alone, and why.
    Words(Option<String>),
}

/// What a search came to.
struct Outcome {
    found: Vec<Found>,
    /// Files the model was unsure of: index and probability.
    maybe: Vec<(usize, f64)>,
    how: How,
}

/// Search, print what was found, and exit 1 when nothing was.
pub(crate) fn grep(args: &GrepArgs) -> Result<i32, Failure> {
    let started = Instant::now();
    let mut terms = terms::terms(&args.query);
    terms.truncate(rank::MOST_TERMS);
    let files = listing::listed(&args.paths)?;
    let docs = read_all(&files)?;
    let definition = terms::asks_for_definition(&args.query);
    let (ranked, weights) = rank::ranked(&docs, &terms, definition);
    let decider = (!args.lexical)
        .then(|| providers::decider(&args.model))
        .flatten();
    let outcome = match decider {
        Some(decider) => {
            let candidates = candidates(&docs, &ranked, &terms, args.judge);
            let parts: Vec<(&Doc, judge::Parts)> = (candidates.iter())
                .filter_map(|&doc| docs.get(doc))
                .map(|doc| (doc, best_parts(doc, &terms, &weights, JUDGED_LINES)))
                .collect();
            match judge::judged(decider.as_ref(), &args.query, &parts) {
                // A definition is in one place: the files linked to it are not looked for.
                Ok(verdicts) if verdicts.yes.iter().any(Option::is_some) && definition => {
                    let second = vec![false; candidates.len()];
                    picked(&candidates, &verdicts, &second, &ranked)
                }
                Ok(verdicts) if verdicts.yes.iter().any(Option::is_some) => {
                    let (candidates, verdicts, second) = links::second_round(
                        decider.as_ref(),
                        args,
                        &docs,
                        &terms,
                        candidates,
                        verdicts,
                    );
                    picked(&candidates, &verdicts, &second, &ranked)
                }
                Ok(_) => by_words(
                    &ranked,
                    args,
                    Some("the decision model gave no answer".into()),
                ),
                Err(failure) => by_words(&ranked, args, failure.message()),
            }
        }
        None if args.lexical => by_words(&ranked, args, None),
        None => by_words(&ranked, args, Some(providers::how_to_decide())),
    };
    let outcome = with_blocks(outcome, &docs, &terms, &weights, args);
    shown::print(&outcome, &docs, args, files.len(), started.elapsed())?;
    Ok(if outcome.found.is_empty() {
        exit::FAILED
    } else {
        exit::OK
    })
}

/// The files the model reads: the best by their words first, then the rest, at most `judge` of
/// them, or every one but the tests when there are no more than half again as many. A test
/// answers a question about the code too rarely to take a place, unless the question is about
/// tests.
fn candidates(docs: &[Doc], ranked: &[rank::Scored], terms: &[Term], judge: usize) -> Vec<usize> {
    let tests_asked = asks_about_tests(terms);
    let kept = |doc: &usize| tests_asked || docs.get(*doc).is_some_and(|d| !rank::is_test(&d.path));
    let mut seen = vec![false; docs.len()];
    let mut order: Vec<usize> = ranked
        .iter()
        .map(|scored| scored.doc)
        .filter(kept)
        .collect();
    order.iter().for_each(|&doc| seen[doc] = true);
    let rest: Vec<usize> = (0..docs.len())
        .filter(|&doc| !seen[doc])
        .filter(kept)
        .collect();
    if order.len() + rest.len() <= judge + judge / 2 {
        order.extend(rest);
        return order;
    }
    order.truncate(judge);
    order
}

/// The least probability a file is shown with, in a round whose best file has `best`: at least
/// [`YES`], and within [`BELOW_BEST`] of the best (the model leans to yes on a file near the
/// subject: one far below the best is seldom what was asked).
fn bar(best: f64) -> f64 {
    YES.max(best - BELOW_BEST)
}

/// Whether the question is about tests: then a test file is read as any other.
fn asks_about_tests(terms: &[Term]) -> bool {
    terms
        .iter()
        .any(|term| term.finds("test") || term.finds("tests"))
}

/// The files the model judged likely to answer, most likely first, and those it was unsure of.
/// A file the `second` round read comes after those the first kept: it was found through one of
/// them, and is told so, which leans the model to yes.
fn picked(
    candidates: &[usize],
    verdicts: &judge::Verdicts,
    second: &[bool],
    ranked: &[rank::Scored],
) -> Outcome {
    let place = |doc: usize| {
        ranked
            .iter()
            .position(|scored| scored.doc == doc)
            .unwrap_or(usize::MAX)
    };
    let round = |at: usize| second.get(at).copied().unwrap_or(false);
    let mut judged: Vec<(usize, f64, Lead, bool)> = (candidates
        .iter()
        .zip(&verdicts.yes)
        .zip(&verdicts.lead)
        .enumerate())
    .filter_map(|(at, ((&doc, yes), lead))| Some((doc, (*yes)?, *lead, round(at))))
    .collect();
    judged.sort_by(|a, b| {
        (a.3.cmp(&b.3))
            .then(b.1.total_cmp(&a.1))
            .then(place(a.0).cmp(&place(b.0)))
    });
    // Each round's margin from its own best: a second round's file comes after all the first
    // kept, so the first's best does not bar it.
    let best = |second: bool| {
        (judged.iter())
            .filter(|(.., round)| *round == second)
            .map(|(_, yes, ..)| *yes)
            .fold(0.0, f64::max)
    };
    let (first_bar, second_bar) = (bar(best(false)), bar(best(true)));
    let bar = |second: bool| match second {
        true => second_bar,
        false => first_bar,
    };
    let found = (judged.iter())
        .filter(|(_, yes, _, second)| *yes >= bar(*second))
        .map(|&(doc, score, lead, _)| Found {
            doc,
            score,
            blocks: vec![],
            lead,
        })
        .collect();
    let maybe = (judged.iter())
        .filter(|(_, yes, _, second)| (MAYBE..bar(*second)).contains(yes))
        .take(MOST_UNSURE)
        .map(|&(doc, yes, ..)| (doc, yes))
        .collect();
    Outcome {
        found,
        maybe,
        how: How::Judged {
            model: verdicts.model.clone(),
            cost: verdicts.cost,
            judged: candidates.len(),
        },
    }
}

/// The best files by their words, `--limit` of them, scored against the best one.
fn by_words(ranked: &[rank::Scored], args: &GrepArgs, why: Option<String>) -> Outcome {
    let best = ranked.first().map_or(1.0, |scored| scored.score);
    let found = (ranked.iter().take(args.limit))
        .map(|scored| Found {
            doc: scored.doc,
            score: scored.score / best,
            blocks: vec![],
            lead: None,
        })
        .collect();
    Outcome {
        found,
        maybe: vec![],
        how: How::Words(why),
    }
}

/// Each file shown given its parts, within `--lines` in all: the best file the most.
fn with_blocks(
    mut outcome: Outcome,
    docs: &[Doc],
    terms: &[Term],
    weights: &[f64],
    args: &GrepArgs,
) -> Outcome {
    let shown = outcome.found.len().min(args.limit).max(1);
    let mut left = args.lines;
    for (n, found) in outcome.found.iter_mut().take(args.limit).enumerate() {
        let Some(doc) = docs.get(found.doc) else {
            continue;
        };
        let share = (left / (shown - n).max(1)).clamp(8, 40).min(left);
        let hits = rank::hits(&doc.text, terms);
        found.blocks = blocks::blocks(&doc.text, &hits, weights, share, found.lead);
        if found.blocks.is_empty() {
            let end = doc.text.lines().count().min(share);
            found.blocks = vec![Block {
                start: 0,
                end,
                score: 0.0,
            }]
            .into_iter()
            .filter(|b| b.end > 0)
            .collect();
        }
        left -= found
            .blocks
            .iter()
            .map(|block| block.end - block.start)
            .sum::<usize>()
            .min(left);
    }
    outcome
}

/// `doc`'s best parts for `terms`, in at most `lines` lines, each headed with its lines.
fn best_parts(doc: &Doc, terms: &[Term], weights: &[f64], lines: usize) -> judge::Parts {
    let hits = rank::hits(&doc.text, terms);
    let text: Vec<&str> = doc.text.lines().collect();
    let mut parts = judge::Parts {
        text: String::new(),
        spans: vec![],
        via: vec![],
    };
    for block in blocks::blocks(&doc.text, &hits, weights, lines, None) {
        let n = parts.spans.len() + 1;
        let _ = writeln!(
            parts.text,
            "[part {n}: lines {}-{}]",
            block.start + 1,
            block.end
        );
        for line in text.iter().take(block.end).skip(block.start) {
            parts.text.push_str(line);
            parts.text.push('\n');
        }
        parts.spans.push((block.start, block.end));
    }
    parts
}

/// Every file of `files` read, a few at once; those that are not text left out.
fn read_all(files: &[String]) -> Result<Vec<Doc>, Failure> {
    let read = each_at_once(files, READERS, "llx-grep-read", |file| listing::read(file))
        .map_err(|error| Failure::Environment(format!("could not start a thread: {error}")))?;
    Ok(read.into_iter().flatten().flatten().collect())
}

#[cfg(test)]
#[path = "grep_tests.rs"]
mod tests;
