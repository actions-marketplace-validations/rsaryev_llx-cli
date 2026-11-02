//! A command `bash` hands to the background (B9), kept as a job: listed and stopped as the
//! others are, waited for on a thread of the lane's scope, its end posted once to the lane's queue
//! with its report (`arrivals::background`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::Scope;
use std::time::{Duration, Instant};

use tools::shell::{Adopted, Background, Running};

use super::{Job, Jobs, What};

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
            state.jobs.push(Job {
                id: id.clone(),
                name: name.clone(),
                what: What::Shell {
                    command: command.to_string(),
                    started,
                    until,
                },
                stop: Arc::clone(&stop),
                _known: jobs.known(&id, &stop),
            });
            Adopted { id, name }
        };
        jobs.show();
        let (id, label) = (adopted.id.clone(), adopted.name.clone());
        let thread = std::thread::Builder::new().name("llx-background".into());
        let waited = thread.spawn_scoped(self.scope, move || {
            let stopped = || stop.load(Ordering::SeqCst);
            let report = running.report(until, &stopped);
            // Posted while it is still listed: `exec` waits for as long as it is.
            if !jobs.over.load(Ordering::SeqCst) {
                (jobs.kit.queue).post(crate::arrivals::background(&label, &report));
            }
            jobs.lock().jobs.retain(|job| job.id != id);
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

#[cfg(test)]
#[path = "shells_tests.rs"]
mod tests;
