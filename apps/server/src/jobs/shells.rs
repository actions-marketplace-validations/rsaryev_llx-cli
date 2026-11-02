//! A command `bash` hands to the background (B9), kept as a job: listed and stopped as the
//! others are, waited for on a thread of the lane's scope, its end posted once to the lane's queue
//! with its report (`arrivals::background`).

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::Scope;
use std::time::{Duration, Instant};

use protocol::{JobEnd, JobKind};
use tools::shell::{Adopted, Background, Running};

use super::ended::{self, Of};
use super::stop::{By, Stop};
use super::{Job, Jobs, What, outputs};

/// How long a command moved to the background may run on, in a session.
const SESSION_LIMIT: Duration = Duration::from_secs(30 * 60);
/// How long one may in `exec`, whose task waits for it.
const EXEC_LIMIT: Duration = Duration::from_secs(10 * 60);
/// Commands in the background at once, at most.
const MAX_SHELLS: usize = 8;

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
        let (stop, started) = (Arc::new(Stop::default()), Instant::now());
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
            let report = running.report(until, &|| stop.asked());
            let over = jobs.over.load(Ordering::SeqCst);
            // Posted while it is still listed: `exec` waits for as long as it is. A stop the
            // model asked for, it knows of (B10).
            let entry = crate::arrivals::background(&label, &report);
            match (over, stop.by()) {
                (true, _) | (false, Some(By::Run)) => {}
                (false, Some(By::Person)) => {
                    jobs.kit
                        .queue
                        .post(crate::arrivals::with_the_next_task(entry));
                }
                (false, None) => jobs.kit.queue.post(entry),
            }
            jobs.lock().jobs.retain(|job| job.id != id);
            // The person may look at how it ended and what it printed.
            if !over {
                let (end, printed) = ended_with(&report, stop.by());
                let of = Of {
                    id: &id,
                    name: &label,
                    kind: JobKind::Shell,
                    detail: &command,
                };
                ended::keep(jobs, &of, end, printed);
            }
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

/// How a command ended, as its row says it (its report's first line, without the reason that is
/// the model's: `Timed out after: 1800.0 seconds`), and what it printed: from its report.
fn ended_with(report: &str, by: Option<By>) -> (JobEnd, String) {
    let head = report.lines().next().unwrap_or_default();
    let how = match by {
        // The report says the person stopped it: the model did, by `jobs stop`.
        Some(By::Run) => "Stopped: by the agent",
        Some(By::Person) | None => head.split_once(" (").map_or(head, |(how, _)| how),
    };
    let printed = report.split_once("\nOutput:\n").map_or("", |(_, out)| out);
    let end = JobEnd {
        failed: how != "Exit code: 0",
        how: how.to_string(),
    };
    (end, printed.to_string())
}

#[cfg(test)]
#[path = "shells_tests.rs"]
mod tests;
