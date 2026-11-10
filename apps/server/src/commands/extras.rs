//! The small commands: `/diff` (the working tree's changes, new files too), `/init` (an
//! AGENTS.md written for the project), `/copy` (the last answer, for the person's clipboard) and
//! `/usage` (what the session spent).

use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

use tools::shell::Unanswered;

use crate::compose::Rig;

/// Rows of a diff shown at most; the rest is counted.
const DIFF_ROWS: usize = 400;
/// How long each git query may take (R10): past it, it is killed and /diff says so. A large
/// repository's diff takes a few seconds; a credential helper or a lock that hangs, forever.
const GIT_WAIT: Duration = Duration::from_secs(10);

/// What `/init` asks.
pub(crate) const INIT: &str = "Write an AGENTS.md at the root of this repository for coding agents \
working in it: what the project is, how it is laid out (the main directories and what lives \
where), how to build, test, lint and run it (the exact commands, checked by running them where \
that is quick and harmless), the conventions the code follows (style, naming, tests, commits), \
and anything a newcomer would get wrong. Read before you write: the README, the manifests, the \
CI configuration, a few representative files. Keep it short and concrete, every line something \
an agent will act on. If an AGENTS.md or CLAUDE.md exists, improve it instead of replacing it.";

/// `/diff`: what changed since the last commit, and the files git does not track yet.
pub(crate) fn diff(root: &Path) -> String {
    compared(root, GIT_WAIT)
}

/// [`diff`], each git query given `wait` (C2: a test sets it).
fn compared(root: &Path, wait: Duration) -> String {
    let git = |command: &str| {
        let mut git = tools::shell::shell(command);
        git.current_dir(root);
        let printed = tools::shell::query(git, wait)?;
        Ok(String::from_utf8_lossy(&printed).into_owned())
    };
    let changed = match git("git diff HEAD --no-color") {
        // No commit yet: the working tree against the index.
        Err(Unanswered::Ended(_)) => git("git diff --no-color"),
        answered => answered,
    };
    let changed = match changed {
        Ok(changed) => changed,
        // Not asked again: a git that hangs here would hang there too.
        Err(late @ Unanswered::Late(_)) => return format!("git {late}: run git diff instead.\n"),
        Err(Unanswered::Start(_) | Unanswered::Lost(_) | Unanswered::Ended(_)) => {
            return "Not a git repository here: nothing to compare.\n".into();
        }
    };
    let untracked = git("git ls-files --others --exclude-standard").unwrap_or_default();
    let mut rows: Vec<String> = changed.lines().map(str::to_string).collect();
    rows.extend(
        untracked
            .lines()
            .map(|path| format!("new file (untracked): {path}")),
    );
    if rows.is_empty() {
        return "No changes since the last commit.\n".into();
    }
    let more = rows.len().saturating_sub(DIFF_ROWS);
    rows.truncate(DIFF_ROWS);
    if more > 0 {
        rows.push(format!("… {more} more rows: run git diff to see them all"));
    }
    rows.join("\n") + "\n"
}

/// `/copy`: the last answer, for the interface to copy: the clipboard is the person's (K2), and
/// the interface says what came of it. What there is to say here instead, if anything.
pub(crate) fn copy(rig: &Rig) -> Option<&'static str> {
    let Some(answer) = last_answer(&rig.session.turns) else {
        return Some("No answer to copy yet.\n");
    };
    match &rig.ui {
        Some(ui) => ui.send(protocol::Event::Copy(answer)),
        None => return Some("No interface to copy to.\n"),
    }
    None
}

/// The last answer that says something: a failed or stopped task leaves none, and copying
/// nothing would empty the clipboard.
fn last_answer(turns: &[contracts::history::Turn]) -> Option<String> {
    (turns.iter().rev())
        .map(|turn| turn.answer.trim())
        .find(|answer| !answer.is_empty())
        .map(str::to_string)
}

/// `/usage`: what the session's tasks spent, the models' turns and tokens and the time.
pub(crate) fn usage(rig: &Rig) -> String {
    use crate::output::human::{thousands, tokens_in};
    let spent = &rig.spent;
    let minutes = rig.begun.elapsed().as_secs() / 60;
    let session = rig.output.total();
    let mut shown = format!(
        "tasks      {}\nagent      {} · {} turns · {} · {} out · ${:.4}\n\
         time       {minutes} min\n\
         session    {} · {} out · ${:.4}\n",
        rig.session.turns.len(),
        rig.choice.model(),
        spent.calls,
        tokens_in(spent.input_tokens, spent.cached_tokens),
        thousands(spent.output_tokens),
        spent.cost,
        tokens_in(session.input_tokens, session.cached_tokens),
        thousands(session.output_tokens),
        session.cost,
    );
    if !rig.session.turns.is_empty() {
        let _ = writeln!(
            shown,
            "\nthe tasks before this one, in detail (`llx spend {}`):",
            rig.session.id
        );
        shown.push_str(&crate::spend::report(&rig.session));
    }
    shown
}

#[cfg(test)]
#[path = "extras_tests.rs"]
mod tests;
