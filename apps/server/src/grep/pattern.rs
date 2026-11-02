//! `llx grep -e`: patterns, as grep's, that keep only the files they match before the question
//! ranks them. A pattern is one term more of the search: its lines count as a hit, centre the
//! parts shown and the decision model reads, and outweigh every word of the question.

use regex_lite::{Regex, RegexBuilder};

use super::Doc;
use super::terms::Term;
use crate::cli::PatternArgs;
use crate::error::Failure;

/// The patterns of one search, as one regex: any of them.
#[derive(Debug, Clone)]
pub(super) struct Pattern(Regex);

impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Eq for Pattern {}

impl Pattern {
    /// How many times it matches in `line`.
    pub(super) fn count(&self, line: &str) -> u32 {
        u32::try_from(self.0.find_iter(line).count()).unwrap_or(u32::MAX)
    }
}

/// The search's pattern from `-e` (each a regex, or with `-F` a fixed string; `-i` any case), or
/// none. A pattern that is not one is the person's mistake, named.
pub(super) fn compiled(args: &PatternArgs) -> Result<Option<Pattern>, Failure> {
    if args.patterns.is_empty() {
        return Ok(None);
    }
    let each = |pattern: &String| match args.fixed {
        true => regex_lite::escape(pattern),
        false => pattern.clone(),
    };
    let any: Vec<String> = args
        .patterns
        .iter()
        .map(|p| format!("(?:{})", each(p)))
        .collect();
    (RegexBuilder::new(&any.join("|")))
        .case_insensitive(args.ignore_case)
        .build()
        .map(|regex| Some(Pattern(regex)))
        .map_err(|error| {
            Failure::Usage(format!(
                "`{}` is not a pattern ({error}); `-F` takes it as it is",
                args.patterns.join("`, `")
            ))
        })
}

/// With a pattern: the files it matches alone, and the pattern as the first term, which no
/// question's word pushes out.
pub(super) fn applied(pattern: Option<&Pattern>, terms: &mut Vec<Term>, docs: &mut Vec<Doc>) {
    let Some(pattern) = pattern else { return };
    docs.retain(|doc| doc.text.lines().any(|line| pattern.count(line) > 0));
    terms.insert(0, Term::pattern(pattern.clone()));
}

/// The pattern's weight raised to the question's heaviest word's: every file has it, so by its
/// rarity alone it would weigh nothing, yet its lines are what was asked for.
pub(super) fn weighed(pattern: Option<&Pattern>, weights: &mut [f64]) {
    if pattern.is_none() {
        return;
    }
    let heaviest = weights.iter().skip(1).copied().fold(1.0, f64::max);
    if let Some(first) = weights.first_mut() {
        *first = heaviest * 2.0;
    }
}

#[cfg(test)]
#[path = "pattern_tests.rs"]
mod tests;
