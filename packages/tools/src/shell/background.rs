//! A command `bash` hands to the background (B9): one asked to start there, or one still running
//! when its wait ends, taken by the session's jobs, which the app gives `bash` as a
//! [`Background`]. Its end comes back to the model on its own.

use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use super::process::Finished;
use super::running::Running;

/// The longest `bash` waits for a command before it goes to the background (B9): a test that
/// hangs costs at most this.
pub(super) const WAIT_MS: u64 = 120_000;
/// The longest a command started in the background may be given to run.
pub(super) const MAX_BACKGROUND_MS: u64 = 7_200_000;

/// The session's jobs, as `bash` reaches them: where a command that runs on goes.
pub trait Background: Send + Sync {
    /// `running` (`command`) taken as a job named after `name`, kept at most `limit` (the
    /// session's own limit when `None`); the job, or `None` when the session takes no more jobs.
    fn adopt(
        &self,
        name: &str,
        command: &str,
        running: Running,
        limit: Option<Duration>,
    ) -> Option<Adopted>;
}

/// A job a command became.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adopted {
    pub id: String,
    pub name: String,
}

/// What `bash` asks of the background for one command.
pub(super) struct Hand<'a> {
    pub command: &'a str,
    /// The job's name, as the model gave it.
    pub name: Option<&'a str>,
    pub timeout_ms: Option<u64>,
    /// Started in the background at once (`run_in_background`), not waited for first.
    pub at_once: bool,
    /// Whether the person stopped the task: a command waited for is killed when it says so.
    pub stop: &'a (dyn Fn() -> bool + Sync),
}

/// What became of a command handed over.
pub(super) enum Handed {
    /// It ended while waited for (or was killed): reported as any command.
    Ended(Finished),
    /// It is a job now: what the model reads of it.
    Job(String),
    /// It could not become one, and was killed: why.
    Refused(String),
}

/// `running` waited for, or handed to `jobs`, as `hand` says.
pub(super) fn hand(
    jobs: &dyn Background,
    mut running: Running,
    hand: &Hand<'_>,
) -> io::Result<Handed> {
    let name = name(hand.name, hand.command);
    if hand.at_once {
        let file = running.keep_whole();
        let limit =
            (hand.timeout_ms).map(|ms| Duration::from_millis(ms.clamp(1, MAX_BACKGROUND_MS)));
        return Ok(match jobs.adopt(&name, hand.command, running, limit) {
            Some(job) => Handed::Job(started(&job, file.as_deref())),
            None => Handed::Refused(FULL.into()),
        });
    }
    let wait = Duration::from_millis(hand.timeout_ms.unwrap_or(WAIT_MS).clamp(1, WAIT_MS));
    if let Some(finished) = running.until(Instant::now() + wait, hand.stop)? {
        return Ok(Handed::Ended(finished));
    }
    // Waiting is the jobs' to do: a `sleep` that outlasts its wait is not kept running for it.
    if sleeps(hand.command) {
        return running
            .run_out(Instant::now(), hand.stop)
            .map(Handed::Ended);
    }
    let (file, so_far, elapsed) = (running.keep_whole(), running.so_far(), running.elapsed());
    match jobs.adopt(&name, hand.command, running, None) {
        Some(job) => Ok(Handed::Job(moved(&job, elapsed, file.as_deref(), &so_far))),
        None => Ok(Handed::Refused(FULL.into())),
    }
}

/// Why a command could not become a job.
const FULL: &str = "the session takes no more jobs: `jobs list` shows them, `jobs stop` ends \
one; the command was killed with everything it started";

/// What waiting for a job's end takes.
const IDLE: &str = "Nothing more is needed to learn its end: no monitor, poll or sleep. Do other \
independent work; if there is none, respond without a tool call and wait.";

/// What the model reads of a command started in the background.
fn started(job: &Adopted, file: Option<&Path>) -> String {
    let mut out = vec![format!(
        "Running in the background: job `{}` ({}); its end comes back to you on its own as a \
         <background> message. {IDLE} `jobs stop` ends it.",
        job.name, job.id
    )];
    out.extend(whole(file));
    out.join("\n")
}

/// What the model reads of a command moved to the background, still running after its wait.
fn moved(job: &Adopted, elapsed: Duration, file: Option<&Path>, so_far: &str) -> String {
    let mut out = vec![format!(
        "Still running after: {:.1} seconds (moved to the background as job `{}` ({}); its end \
         comes back to you on its own as a <background> message. {IDLE} It runs now: do not \
         start a second copy. A command that should have ended by now may be stuck, looping or waiting, \
         rather than slow: if so, end it with `jobs stop` and find why.)",
        elapsed.as_secs_f64(),
        job.name,
        job.id
    )];
    out.extend(whole(file));
    out.push("Output so far:".into());
    if !so_far.is_empty() {
        out.push(so_far.into());
    }
    out.join("\n")
}

/// The line naming the file with the whole output, as it grows.
fn whole(file: Option<&Path>) -> Option<String> {
    file.map(|file| {
        format!(
            "Whole output: {} (it grows as the command prints; read it with `tail` or `sed -n`)",
            file.display()
        )
    })
}

/// The job's name: the model's, else the command's first words.
fn name(given: Option<&str>, command: &str) -> String {
    let given = given.map(str::trim).filter(|name| !name.is_empty());
    let name = given.map_or_else(
        || {
            command
                .split_whitespace()
                .take(2)
                .collect::<Vec<_>>()
                .join(" ")
        },
        str::to_string,
    );
    name.chars().take(40).collect()
}

/// Whether `command` starts by sleeping.
fn sleeps(command: &str) -> bool {
    let first = command.split_whitespace().next();
    first == Some("sleep")
}

#[cfg(test)]
#[path = "background_tests.rs"]
mod tests;
