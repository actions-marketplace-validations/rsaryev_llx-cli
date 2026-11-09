//! `llx sessions` and `llx resume`: every run is a session, kept as a JSON Lines file of its
//! tasks and how each ended. The conversation itself is not kept, so `resume` starts a new one
//! that is told what the session asked before, and records into the same session.

use std::path::{Path, PathBuf};

use contracts::history::{History, HistoryError, Session};
use session::Sessions;

use crate::cli::RunOptions;
use crate::commands::run::{self, Kind};
use crate::error::{self, Failure};
use crate::exit;
use crate::printable::{clipped, one_line, printable};

/// Sessions listed at most.
const LISTED: usize = 30;
/// Characters of a session's first task in the list.
const TASK_CHARS: usize = 60;

/// Where sessions are kept: `LLX_SESSIONS`, else `~/.llx/sessions`.
pub(crate) fn store() -> Sessions {
    let dir = match std::env::var_os("LLX_SESSIONS").filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".llx/sessions"),
    };
    Sessions { dir }
}

pub(crate) fn list() -> Result<i32, Failure> {
    let all = store().list();
    let (agents, all): (Vec<Session>, Vec<Session>) = all.into_iter().partition(Session::is_agent);
    if all.is_empty() {
        error::print("No saved sessions.\n")?;
        return Ok(exit::OK);
    }
    let rows: String = all
        .iter()
        .take(LISTED)
        .map(|session| row(session, children(session, &agents)))
        .collect();
    error::print(&rows)?;
    Ok(exit::OK)
}

/// How many of `agents` `session` started.
fn children(session: &Session, agents: &[Session]) -> usize {
    agents
        .iter()
        .filter(|agent| agent.parent.as_deref() == Some(session.id.as_str()))
        .count()
}

/// One session as `llx sessions` lists it: its id, when, how many tasks, where, the first one,
/// and how many agents it started.
fn row(session: &Session, agents: usize) -> String {
    let shown = first_task(session);
    let place = match session.workspace.as_str() {
        "" => "?".to_string(),
        dir => printable(&home_relative(dir)),
    };
    let tasks = tasks(session);
    let agents = match agents {
        0 => String::new(),
        n => format!("  ({})", crate::output::count(n, "agent")),
    };
    format!(
        "{}  {:>10}  {tasks:>8}  {place}  {shown}{agents}\n",
        session.id,
        ago(session.updated)
    )
}

/// A session as the interface's list shows it: its first task, and when it ran and how many tasks
/// it has (`2 h ago · 4 tasks`).
pub(crate) fn brief(session: &Session) -> (String, String) {
    let ago = ago(session.updated);
    let tasks = crate::output::count(session.turns.len(), "task");
    (first_task(session), format!("{ago} · {tasks}"))
}

/// `3 min ago`, `2 h ago`, `5 d ago`.
fn ago(at: u64) -> String {
    let seconds = crate::clock::now().saturating_sub(at);
    match seconds {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", seconds / 60),
        3600..86400 => format!("{} h ago", seconds / 3600),
        _ => format!("{} d ago", seconds / 86400),
    }
}

/// `1 task `, `4 tasks`: the same width, for a column.
fn tasks(session: &Session) -> String {
    match session.turns.len() {
        1 => "1 task ".to_string(),
        n => format!("{n} tasks"),
    }
}

/// The session's title, else its first task on one line, cut.
fn first_task(session: &Session) -> String {
    if !session.title.is_empty() {
        return printable(&session.title);
    }
    let first = session.turns.first().map_or("", |turn| turn.goal.as_str());
    clipped(&printable(&one_line(first)), TASK_CHARS)
}

/// The sessions of `list` that ran in `dir`, newest first, without the agents'.
pub(crate) fn here(mut list: Vec<Session>, dir: &str) -> Vec<Session> {
    list.retain(|session| session.workspace == dir && !session.is_agent());
    list.sort_by_key(|session| std::cmp::Reverse(session.updated));
    list
}

/// `dir` with the home directory written `~`, as a person recognizes it.
pub(crate) fn home_relative(dir: &str) -> String {
    let home = std::env::var_os("HOME");
    let inside = home
        .as_deref()
        .map(Path::new)
        .filter(|home| !home.as_os_str().is_empty())
        .and_then(|home| Path::new(dir).strip_prefix(home).ok());
    match inside {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => Path::new("~").join(rest).display().to_string(),
        None => dir.to_string(),
    }
}

/// `llx resume` and `llx exec resume`: the session `id` names, else the newest one that ran in
/// this workspace (else the newest of all, said so), going on with `prompt` as `kind`.
pub(crate) fn resume(
    id: Option<&str>,
    prompt: Option<&str>,
    options: RunOptions,
    kind: Kind,
    front: &'static dyn run::Front,
) -> Result<i32, Failure> {
    let root = run::root(&options)?;
    let here = root.root().display().to_string();
    let store = store();
    let session = match id {
        Some(id) => store.load(id),
        None => {
            // Read once, newest first: the newest here, else the newest of all; an agent's
            // session goes on only through its parent.
            let mut all = store.list();
            let ran_here = |s: &Session| s.workspace == here && !s.is_agent();
            let at =
                (all.iter().position(ran_here)).or_else(|| all.iter().position(|s| !s.is_agent()));
            at.map(|at| all.swap_remove(at)).ok_or(HistoryError::Empty)
        }
    };
    let session = session
        .map_err(|why| Failure::Usage(format!("{why}: `llx sessions` lists the saved ones")))?;
    if !session.workspace.is_empty() && session.workspace != here {
        error::note(&format!(
            "llx: session {} ran in {}; this one runs in {here}\n",
            session.id,
            printable(&session.workspace)
        ));
    }
    run::run(prompt, options, Some(session), kind, front)
}

/// `llx spend`: what the session `id` names spent, else the newest one of this directory (else
/// the newest of all), on stdout.
pub(crate) fn spend(id: Option<&str>) -> Result<i32, Failure> {
    let store = store();
    let session = match id {
        Some(id) => store.load(id),
        None => {
            let here = std::env::current_dir()
                .map(|dir| dir.display().to_string())
                .unwrap_or_default();
            let mut all = store.list();
            let ran_here = |s: &Session| s.workspace == here && !s.is_agent();
            let at =
                (all.iter().position(ran_here)).or_else(|| all.iter().position(|s| !s.is_agent()));
            at.map(|at| all.swap_remove(at)).ok_or(HistoryError::Empty)
        }
    };
    let session = session
        .map_err(|why| Failure::Usage(format!("{why}: `llx sessions` lists the saved ones")))?;
    error::print(&crate::spend::report(&session))?;
    Ok(exit::OK)
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
