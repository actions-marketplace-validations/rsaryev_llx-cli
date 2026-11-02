//! `llx grep`, the command: the decision model its options name, the search, and what it found
//! printed; exit 1 when no file answers.

use std::path::Path;
use std::time::Instant;

use super::{Ask, Reader, search, shown};
use crate::cli::GrepArgs;
use crate::error::Failure;
use crate::exit;
use crate::providers;

/// Search, print what was found, and exit 1 when nothing was.
pub(crate) fn grep(args: &GrepArgs) -> Result<i32, Failure> {
    let started = Instant::now();
    let decider = (!args.lexical)
        .then(|| providers::decider(&args.model))
        .flatten();
    let reader = match &decider {
        Some(decider) => Reader::Model(decider.as_ref()),
        None if args.lexical => Reader::Words(None),
        None => Reader::Words(Some(providers::how_to_decide())),
    };
    let ask = Ask {
        query: &args.query,
        paths: &args.paths,
        base: Path::new("."),
        pattern: &args.pattern,
        limit: args.limit,
        lines: args.lines,
        judge: args.judge,
        readable: &|_| true,
    };
    let searched = search(&ask, reader)?;
    let (outcome, docs) = (&searched.outcome, &searched.docs);
    shown::print(outcome, docs, args, searched.listed, started.elapsed())?;
    Ok(if outcome.found.is_empty() {
        exit::FAILED
    } else {
        exit::OK
    })
}
