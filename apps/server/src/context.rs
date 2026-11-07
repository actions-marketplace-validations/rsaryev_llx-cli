//! What the agent knows of where it runs before its first call: the working directory, the
//! system, the runtimes its shell has, the date, the mode it works in, and the instructions it
//! follows (see [`crate::instructions`]); for a resumed session, what it did before. It is the
//! same for every task and session in a workspace on a day, so the prompt cache keeps it: nothing
//! in it changes by the minute.

use std::path::{Path, PathBuf};

use contracts::history::Session;

use crate::instructions;

/// The runtimes a script can be written in, named in this order when the shell has them.
const RUNTIMES: [&str; 3] = ["python3", "node", "bun"];

/// Bytes of an earlier session's answers a resumed session is told.
const PREVIOUS_LIMIT: usize = 8 * 1024;
/// Bytes of one earlier answer.
const ANSWER_LIMIT: usize = 1500;
/// Bytes of one earlier task: a goal may carry a whole piped file in its `<stdin>` block.
const GOAL_LIMIT: usize = 500;

/// Where the run happens, as the context block's first part says it.
pub(crate) struct Environment<'a> {
    /// The workspace root (canonical).
    pub root: &'a Path,
    /// The mode: what runs without asking, what is refused, whether questions are answered.
    pub mode: &'a str,
    /// Unix seconds now.
    pub now: u64,
    /// The runtimes the session's shell has ([`runtimes`]).
    pub runtimes: &'a [&'a str],
}

/// The whole context: the environment and the project's instructions.
pub(crate) fn gather(environment: &Environment) -> String {
    let mut parts = vec![describe(environment)];
    let instructions = instructions(environment.root);
    if !instructions.is_empty() {
        parts.push(instructions);
    }
    parts.join("\n\n")
}

/// The context `fresh` going on from `session`, with what it did before: a resumed run, or a
/// saved session opened in a lane of its own.
pub(crate) fn resumed(fresh: &str, session: &Session, root: &Path) -> String {
    match session.turns.is_empty() {
        true => fresh.to_string(),
        false => format!("{fresh}\n\n{}", earlier(session, root)),
    }
}

/// The `<environment>` block. The shell is not said here: the `bash` tool's own description
/// names it.
fn describe(environment: &Environment) -> String {
    let runtimes = match environment.runtimes {
        [] => String::new(),
        found => format!(
            "Runtimes: {} (for a script: use whichever makes the job simplest)\n",
            found.join(", ")
        ),
    };
    format!(
        "<environment>\nWorking directory: {} (the workspace root)\n\
         System: {} {}\n{runtimes}Date: {} (UTC)\nMode: {}\n</environment>",
        environment.root.display(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        date(environment.now),
        environment.mode,
    )
}

/// The runtimes found in the directories of `path` (a `PATH`), looked up without running
/// anything (ARCHITECTURE.md B2): a program there that can be run.
pub(crate) fn runtimes(path: &std::ffi::OsStr) -> Vec<&'static str> {
    let dirs: Vec<PathBuf> = std::env::split_paths(path).collect();
    (RUNTIMES.into_iter())
        .filter(|name| dirs.iter().any(|dir| runnable(&dir.join(name))))
        .collect()
}

fn runnable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// The person's own instructions, then the project's from the repository root down to `root`,
/// each in its own block, capped.
fn instructions(root: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut blocks = vec![];
    if let Some(own) = instructions::personal(home.as_deref()) {
        blocks.push(instructions::block("personal_instructions", &own));
    }
    let project = instructions::project(root);
    if project.is_empty() {
        // Said, so the model does not walk the disk for a file that is not there.
        blocks.push(NONE_FOUND.to_string());
    }
    for found in project {
        blocks.push(instructions::block("project_instructions", &found));
    }
    blocks.join("\n\n")
}

/// What the context says when no project instruction file applies.
const NONE_FOUND: &str = "No project instructions: no AGENTS.md or CLAUDE.md from the repository's \
    root down to the working directory.";

/// What an earlier session asked and how each task ended, newest kept when it is long, and where
/// it ran when that is not `root`.
fn earlier(session: &Session, root: &Path) -> String {
    let mut left = PREVIOUS_LIMIT;
    let mut turns = vec![];
    for (number, turn) in session.turns.iter().enumerate().rev() {
        let answer = cut(turn.answer.trim(), ANSWER_LIMIT);
        let entry = format!(
            "{}. Task: {}\n   Ended: {}\n   Answer: {answer}",
            number + 1,
            goal(&turn.goal),
            turn.status
        );
        // Each entry is capped, so an older one may still fit after a newer did not.
        if entry.len() > left {
            continue;
        }
        left -= entry.len();
        turns.push(entry);
    }
    turns.reverse();
    let here = root.display().to_string();
    let place = match session.workspace.as_str() {
        "" => String::new(),
        dir if dir == here => format!("Its tasks ran in this workspace, {dir}.\n"),
        dir => format!(
            "Its tasks ran in {dir}, not in this workspace: what they changed is there, not here.\n"
        ),
    };
    format!(
        "<previous_session id=\"{}\">\nThis continues an earlier session. Its conversation was \
         not kept, only each task and how it ended; check the current state with the tools.\n\
         {place}{}\n</previous_session>",
        session.id,
        turns.join("\n")
    )
}

/// An earlier task as the context tells it: its `<stdin>` block left out, and capped.
fn goal(goal: &str) -> String {
    let asked = match goal.split_once("\n\n<stdin>\n") {
        Some((asked, _)) => format!("{} [with stdin, not kept here]", asked.trim()),
        None => goal.trim().to_string(),
    };
    match cut(&asked, GOAL_LIMIT) {
        kept if kept.len() < asked.len() => format!("{kept}…"),
        kept => kept.to_string(),
    }
}

/// At most `limit` bytes of `text`, cut on a character boundary.
pub(crate) fn cut(text: &str, limit: usize) -> &str {
    let end = (0..=limit.min(text.len()))
        .rev()
        .find(|at| text.is_char_boundary(*at));
    text.get(..end.unwrap_or(0)).unwrap_or_default()
}

/// The civil date of `days` since the Unix epoch: year, month, day (Howard Hinnant's
/// civil-from-days algorithm: no date crate needed).
pub(crate) fn civil(days: u64) -> (i64, u32, u32) {
    // A count of days is far below i64::MAX: the cast keeps it.
    let z = days.cast_signed() + 719_468;
    let era = z / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    // The algorithm keeps them in 1..=31 and 1..=12.
    let day = u32::try_from(day_of_year - (153 * shifted_month + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    })
    .unwrap_or(1);
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}

/// `2026-10-22` for Unix seconds, in UTC.
pub(crate) fn date(unix: u64) -> String {
    let (year, month, day) = civil(unix / 86_400);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
