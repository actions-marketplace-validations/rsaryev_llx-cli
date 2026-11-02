//! `llx grep`'s command line: the question, where to look, and grep's patterns.

use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub(crate) struct GrepArgs {
    /// What to find, in words or names: a question, or the names it is about.
    pub query: String,
    /// Where to search: files and directories (the current one when none).
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,
    #[command(flatten)]
    pub pattern: PatternArgs,
    /// The paths alone, best first, a line each.
    #[arg(short = 'l', long, conflicts_with = "json")]
    pub files: bool,
    /// One JSON object: each file with its score and parts, what was unsure, how they were
    /// picked and the cost.
    #[arg(long)]
    pub json: bool,
    /// At most this many files shown with their lines; the rest are named.
    #[arg(short = 'n', long, value_name = "N", default_value_t = 10)]
    pub limit: usize,
    /// At most this many lines shown in all, the best file given the most.
    #[arg(long, value_name = "N", default_value_t = 150)]
    pub lines: usize,
    /// By the words alone, even where a decision model is there: no network, no cost.
    #[arg(long)]
    pub lexical: bool,
    /// At most this many files read by the decision model, the best by their words first
    /// (all of them when there are no more).
    #[arg(long, value_name = "N", default_value_t = 80)]
    pub judge: usize,
    /// The decision model.
    #[arg(short = 'm', long, value_name = "MODEL", env = "LLX_DECISION_MODEL", default_value = crate::providers::DECISION_DEFAULT)]
    pub model: String,
}

/// `llx grep -e`: grep's patterns, which keep only the files they match.
#[derive(Args, Debug)]
pub(crate) struct PatternArgs {
    /// Only the files with a line this pattern matches (a regex, as grep's; given again, any of
    /// them), then ranked by the question: `-e 'spawn|shell::query' "what starts a process"`.
    #[arg(short = 'e', long = "regexp", value_name = "PATTERN")]
    pub patterns: Vec<String>,
    /// The patterns as fixed strings, not regexes.
    #[arg(short = 'F', long, requires = "patterns")]
    pub fixed: bool,
    /// The patterns in any case.
    #[arg(short = 'i', long, requires = "patterns")]
    pub ignore_case: bool,
}
