//! A monitor as a job, on its thread (B10): its wait, then its end, given back once by who
//! stopped it: to the model as it ended, with the person's next task when the person stopped it,
//! not at all when the model did or the session ended; then listed as ended for the person.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use protocol::{JobEnd, JobKind};

use super::Jobs;
use super::ended::{self, Of};
use super::monitor::{self, Check, Ended, Waited};
use super::stop::{By, Stop};
use crate::sync::lock;

/// A monitor started, as its thread runs it.
pub(super) struct Watched {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) command: String,
    pub(super) scope: tools::shell::Scope,
    pub(super) every: Duration,
    pub(super) until: Instant,
    pub(super) stop: Arc<Stop>,
    /// Checks run so far, and what the last printed: its row and its view read them.
    pub(super) checks: Arc<AtomicUsize>,
    pub(super) last: Arc<Mutex<String>>,
}

impl Watched {
    /// Wait as it says, then give its end back and list it as ended.
    pub(super) fn run(self, jobs: &Jobs<'_>) {
        let kit = &jobs.kit;
        let started = Instant::now();
        let seen = |count: usize, check: &Check| {
            self.checks.store(count, Ordering::SeqCst);
            lock(&self.last).clone_from(&check.output);
            jobs.show();
        };
        let hidden = (kit.hidden)();
        let watch = monitor::Watch {
            command: &self.command,
            scope: self.scope,
            workspace: kit.workspace,
            hidden: &hidden,
            every: self.every,
            deadline: self.until,
        };
        let waited = monitor::wait(&watch, &|| self.stop.asked(), &seen);
        let end = how(&waited, self.stop.by(), started.elapsed());
        let last = waited.last.unwrap_or_default();
        let entry = crate::arrivals::monitor(&self.name, &end.how, &said(&last));
        let of = Of {
            id: &self.id,
            name: &self.name,
            kind: JobKind::Monitor,
            detail: &self.command,
        };
        ended::finish(jobs, &of, &self.stop, entry, || (end, last.output));
    }
}

/// How a wait ended, as its row and the model's message say it (`met after 4 checks (32 s)`):
/// anything but met is a failure.
fn how(waited: &Waited, by: Option<By>, took: Duration) -> JobEnd {
    let checks = crate::output::count(waited.checks, "check");
    let took = match took.as_secs() {
        seconds @ 0..120 => format!("{seconds} s"),
        seconds => format!("{} min", seconds / 60),
    };
    let how = match (waited.ended, by) {
        (Ended::Met, _) => format!("met after {checks} ({took})"),
        (Ended::TimedOut, _) => format!("timed out after {checks} ({took})"),
        (Ended::Unrunnable, _) => {
            let check = waited.last.as_ref().map_or("", |check| check.how.as_str());
            format!("could not run: {check}")
        }
        (Ended::Stopped, Some(By::Person)) => format!("stopped by the person after {checks}"),
        (Ended::Stopped, Some(By::Run) | None) => format!("stopped by the agent after {checks}"),
    };
    JobEnd {
        failed: waited.ended != Ended::Met,
        how,
    }
}

/// What the model reads of the last check: how it ended and what it printed.
fn said(last: &Check) -> String {
    match (last.how.is_empty(), last.output.is_empty()) {
        (true, _) => "(no check ran)".into(),
        (false, true) => format!("{}\n(no output)", last.how),
        (false, false) => format!("{}\n{}", last.how, last.output),
    }
}

#[cfg(test)]
#[path = "watched_tests.rs"]
mod tests;
