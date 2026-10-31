//! `jobs`: work that goes on in the background and comes back to the conversation later. A
//! monitor runs a command and gives each batch of lines it prints to the main agent: at its next
//! step while a task runs, else as a new task. A schedule gives its prompt as a task of its own
//! when due, between tasks, behind what the person queued. Both live as long as the session (a
//! monitor until its deadline); in `exec` a task waits for its monitors, and schedules are
//! refused: the process ends with the task.

mod cron;
mod monitor;
pub(crate) mod spec;
mod tool;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use contracts::gate::Gate;
use tools::workspace::Workspace;

pub(crate) use self::tool::JobsTool;
use crate::output::Output;
use crate::sync::Bell;
use protocol::{Event, JobRow};

/// What the jobs are built from.
pub(crate) struct Kit<'a> {
    pub workspace: &'a Workspace,
    pub gate: &'a dyn Gate,
    /// What commands never see, read as each monitor starts.
    pub hidden: &'a (dyn Fn() -> Vec<String> + Sync),
    pub output: &'a Output,
    /// A session: idle time to run what comes back in, and schedules.
    pub session: bool,
    /// The lane's queue: what the jobs give back is posted there.
    pub queue: &'a queue::Queue,
}

/// The jobs of a session, shared by the tool, the monitors' threads and the scheduler.
pub(crate) struct Jobs<'a> {
    kit: Kit<'a>,
    state: Mutex<State>,
    /// The session is over: every job ends.
    over: AtomicBool,
    /// The session's id: its jobs' stops are known under it (every session has a `job-1`).
    owner: String,
    /// Wakes the scheduler: a schedule added or stopped, the session over.
    bell: Arc<Bell>,
}

/// The longest the scheduler waits for a schedule unwoken: the wall clock may jump (the machine
/// slept), and a schedule is due by it.
const LONGEST: Duration = Duration::from_secs(60);

#[derive(Default)]
struct State {
    jobs: Vec<Job>,
    made: u32,
}

struct Job {
    id: String,
    name: String,
    what: What,
    stop: Arc<AtomicBool>,
    /// Its stop, known to the interface's keys while it is listed.
    _known: crate::interrupt::Known,
}

enum What {
    Monitor {
        command: String,
        until: Instant,
        /// Batches it gave.
        events: usize,
    },
    Schedule {
        prompt: String,
        cron: Option<cron::Cron>,
        /// Unix seconds.
        next: u64,
        expires: u64,
    },
}

impl<'a> Jobs<'a> {
    /// The jobs of session `owner`, built from `kit`.
    pub(crate) fn new(owner: &str, kit: Kit<'a>) -> Self {
        Self {
            kit,
            state: Mutex::default(),
            over: AtomicBool::new(false),
            owner: owner.to_string(),
            bell: Arc::default(),
        }
    }

    /// The lane's queue, where what the jobs give back is posted.
    pub(crate) fn queue(&self) -> &queue::Queue {
        self.kit.queue
    }

    /// Know `stop` as job `id`'s stop, under the session's id.
    fn known(&self, id: &str, stop: &Arc<AtomicBool>) -> crate::interrupt::Known {
        let key = crate::interrupt::job_key(&self.owner, id);
        let (stop, bell) = (Arc::clone(stop), Arc::clone(&self.bell));
        crate::interrupt::register(&key, move || {
            stop.store(true, Ordering::SeqCst);
            bell.ring();
            false
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        crate::sync::lock(&self.state)
    }

    /// The task ended: in `exec` its monitors end (what they gave back goes by its kind).
    pub(crate) fn end(&self) {
        if !self.kit.session {
            for job in &self.lock().jobs {
                job.stop.store(true, Ordering::SeqCst);
            }
        }
    }

    /// Whether the task must wait: in `exec`, a monitor still runs.
    pub(crate) fn expecting(&self) -> bool {
        !self.kit.session
            && self
                .lock()
                .jobs
                .iter()
                .any(|job| matches!(job.what, What::Monitor { .. }))
    }

    /// The session is over: every job ends.
    pub(crate) fn close(&self) {
        self.over.store(true, Ordering::SeqCst);
        for job in &self.lock().jobs {
            job.stop.store(true, Ordering::SeqCst);
        }
        self.bell.ring();
    }

    /// The rows the interface lists, sent whenever they change.
    fn show(&self) {
        let Some(ui) = self.kit.output.ui() else {
            return;
        };
        let rows = self.lock().jobs.iter().map(row).collect();
        ui.send(Event::Jobs(rows));
    }

    /// The scheduler: the schedules that are due give their prompt. It sleeps until the soonest
    /// is due, or a schedule is added or stopped, or the session is over.
    pub(crate) fn tick(&self) {
        loop {
            let seen = self.bell.rung();
            if self.over.load(Ordering::SeqCst) {
                return;
            }
            let now = crate::clock::now();
            let (mut due, mut changed, mut soonest) = (vec![], false, None::<u64>);
            let mut state = self.lock();
            state.jobs.retain_mut(|job| {
                let What::Schedule {
                    prompt,
                    cron,
                    next,
                    expires,
                } = &mut job.what
                else {
                    return true;
                };
                if job.stop.load(Ordering::SeqCst) {
                    changed = true;
                    return false;
                }
                if now >= *next {
                    due.push(crate::arrivals::schedule(&job.name, prompt));
                    match cron.as_ref().and_then(|cron| cron.next(now)) {
                        Some(at) if at <= *expires => *next = at,
                        _ => return false,
                    }
                }
                soonest = Some(soonest.map_or(*next, |soonest| soonest.min(*next)));
                true
            });
            drop(state);
            changed |= !due.is_empty();
            for prompt in due {
                self.kit.queue.post(prompt);
            }
            if changed {
                self.show();
            }
            let wait = soonest.map(|next| {
                let left = Duration::from_secs(next.saturating_sub(crate::clock::now()).max(1));
                left.min(LONGEST)
            });
            self.bell.wait(seen, wait);
        }
    }
}

/// A job as the model lists it.
fn line(job: &Job) -> String {
    match &job.what {
        What::Monitor {
            command,
            until,
            events,
        } => format!(
            "- monitor `{}` ({}): `{command}`, {events} batches so far, {} min left",
            job.name,
            job.id,
            until
                .saturating_duration_since(Instant::now())
                .as_secs()
                .div_ceil(60)
        ),
        What::Schedule {
            prompt, cron, next, ..
        } => format!(
            "- schedule `{}` ({}): {}, next at {}: {prompt}",
            job.name,
            job.id,
            cron.as_ref()
                .map_or_else(|| "once".into(), |cron| cron.text().to_string()),
            cron::clock(*next)
        ),
    }
}

/// When `until` comes, in milliseconds since the epoch: the wire carries no `Instant`.
fn epoch_ms(until: Instant) -> u64 {
    let at = std::time::SystemTime::now() + until.saturating_duration_since(Instant::now());
    let since = at.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
}

/// A job as the interface lists it.
fn row(job: &Job) -> JobRow {
    match &job.what {
        What::Monitor {
            command,
            until,
            events,
        } => JobRow {
            id: job.id.clone(),
            name: job.name.clone(),
            detail: command.clone(),
            until_at: Some(epoch_ms(*until)),
            next: None,
            events: *events,
        },
        What::Schedule {
            prompt, cron, next, ..
        } => JobRow {
            id: job.id.clone(),
            name: job.name.clone(),
            detail: match cron {
                Some(cron) => format!("{} · {prompt}", cron.text()),
                None => prompt.clone(),
            },
            until_at: None,
            next: Some(cron::clock(*next)),
            events: 0,
        },
    }
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;
