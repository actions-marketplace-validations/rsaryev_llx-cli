//! A command `bash` hands to the background (B9), kept as a job: listed and stopped as the
//! others are, waited for on a thread of the lane's scope, its end posted once to the lane's queue
//! with its report (`arrivals::background`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::Scope;
use std::time::{Duration, Instant};

use protocol::{JobEnd, JobKind, JobRow};
use tools::shell::{Adopted, Background, Running};

use super::{Job, Jobs, What, outputs};

/// How long a command moved to the background may run on, in a session.
const SESSION_LIMIT: Duration = Duration::from_secs(30 * 60);
/// How long one may in `exec`, whose task waits for it.
const EXEC_LIMIT: Duration = Duration::from_secs(10 * 60);
/// Commands in the background at once, at most.
const MAX_SHELLS: usize = 8;
/// Commands that ended still listed, at most: the latest.
const MAX_ENDED: usize = 8;

/// The session's jobs as `bash` reaches them, their threads in the lane's scope.
pub(crate) struct Shells<'scope, 'env> {
    pub(crate) jobs: &'env Jobs<'env>,
    pub(crate) scope: &'scope Scope<'scope, 'env>,
}

impl Background for Shells<'_, '_> {
    fn adopt(
        &self,
        name: &str,
        command: &str,
        mut running: Running,
        limit: Option<Duration>,
    ) -> Option<Adopted> {
        let jobs = self.jobs;
        let ceiling = match jobs.kit.session {
            true => limit.unwrap_or(SESSION_LIMIT),
            false => limit.unwrap_or(EXEC_LIMIT).min(EXEC_LIMIT),
        };
        let (stop, started) = (Arc::new(AtomicBool::new(false)), Instant::now());
        let until = started + ceiling;
        let adopted = {
            let mut state = jobs.lock();
            let shells = (state.jobs.iter())
                .filter(|job| matches!(job.what, What::Shell { .. }))
                .count();
            if shells >= MAX_SHELLS || jobs.over.load(Ordering::SeqCst) {
                return None;
            }
            let (id, name) = super::new_job(&mut state, Some(name.to_string()));
            let watch = running.watch();
            let key = crate::interrupt::job_key(&jobs.owner, &id);
            state.jobs.push(Job {
                id: id.clone(),
                name: name.clone(),
                what: What::Shell {
                    command: command.to_string(),
                    started,
                    until,
                    _output: outputs::register(&key, move || watch.so_far()),
                },
                stop: Arc::clone(&stop),
                _known: jobs.known(&id, &stop),
            });
            Adopted { id, name }
        };
        jobs.show();
        let (id, label, command) = (
            adopted.id.clone(),
            adopted.name.clone(),
            command.to_string(),
        );
        let thread = std::thread::Builder::new().name("llx-background".into());
        let waited = thread.spawn_scoped(self.scope, move || {
            let stopped = || stop.load(Ordering::SeqCst);
            let report = running.report(until, &stopped);
            let over = jobs.over.load(Ordering::SeqCst);
            // Posted while it is still listed: `exec` waits for as long as it is.
            if !over {
                (jobs.kit.queue).post(crate::arrivals::background(&label, &report));
            }
            let mut state = jobs.lock();
            state.jobs.retain(|job| job.id != id);
            // The person may look at how it ended and what it printed; in `exec` no one does.
            if jobs.kit.session && !over {
                let ended = Ended::of(jobs, (&id, &label, &command), &report);
                state.ended.push(ended);
                let past = state.ended.len().saturating_sub(MAX_ENDED);
                state.ended.drain(..past);
            }
            drop(state);
            jobs.show();
        });
        // The OS refused a thread: the command, let go, is killed with all it started.
        if waited.is_err() {
            jobs.lock().jobs.retain(|job| job.id != adopted.id);
            jobs.show();
            return None;
        }
        Some(adopted)
    }
}

/// A command in the background that ended, listed with how until the person lets it go (`x`)
/// or later ones take its place.
pub(super) struct Ended {
    id: String,
    name: String,
    command: String,
    end: JobEnd,
    /// The person let it go.
    stop: Arc<AtomicBool>,
    _known: crate::interrupt::Known,
    _output: outputs::Shown,
}

impl Ended {
    /// Job `id`, named `name`, that ran `command` and ended with `report` (as `bash` reports).
    fn of(jobs: &Jobs<'_>, (id, name, command): (&str, &str, &str), report: &str) -> Self {
        let head = report.lines().next().unwrap_or_default();
        // The first line's reason (`Timed out after: 1800.0 seconds (killed; …)`) is the model's.
        let how = head
            .split_once(" (")
            .map_or(head, |(how, _)| how)
            .to_string();
        let printed = report.split_once("\nOutput:\n").map_or("", |(_, out)| out);
        let printed: Arc<str> = Arc::from(printed);
        let stop = Arc::new(AtomicBool::new(false));
        let key = crate::interrupt::job_key(&jobs.owner, id);
        Self {
            id: id.to_string(),
            name: name.to_string(),
            command: command.to_string(),
            end: JobEnd {
                failed: how != "Exit code: 0",
                how,
            },
            _known: jobs.known(id, &stop),
            stop,
            _output: outputs::register(&key, move || printed.to_string()),
        }
    }

    /// Its row in the interface's list.
    pub(super) fn row(&self) -> JobRow {
        JobRow {
            id: self.id.clone(),
            name: self.name.clone(),
            kind: JobKind::Shell,
            detail: self.command.clone(),
            until_at: None,
            next: None,
            events: 0,
            ended: Some(self.end.clone()),
        }
    }
}

/// The ended commands the person let go leave the list; whether any did.
pub(super) fn let_go(ended: &mut Vec<Ended>) -> bool {
    let listed = ended.len();
    ended.retain(|ended| !ended.stop.load(Ordering::SeqCst));
    ended.len() != listed
}

#[cfg(test)]
#[path = "shells_tests.rs"]
mod tests;
