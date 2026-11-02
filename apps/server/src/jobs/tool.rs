//! The `jobs` tool: the model starts a monitor or a schedule, lists them and stops them.

use std::fmt::Write as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::Scope;
use std::time::{Duration, Instant};

use contracts::gate::{Effect, Needs};
use contracts::tool::{Input, Tool, ToolError, ToolOutput, ToolSpec};
use serde_json::Value;

use super::monitor::{self, Ended};
use super::spec::{self, Action, When};
use super::{Job, Jobs, State, What, cron};

/// Monitors running at once, at most.
const MAX_MONITORS: usize = 8;
/// Jobs of either kind kept, at most, for a schedule to be added.
const MAX_SCHEDULES: usize = 50;
/// How long a recurring schedule lives.
const WEEK: u64 = 7 * 86_400;
/// Batches a monitor may give before it is stopped as a firehose.
const MAX_EVENTS: usize = 100;
/// What a monitor stopped at `MAX_EVENTS` says to do.
const RESTART: &str = "start it again with a filter that prints only the lines you act on";

pub(crate) struct JobsTool<'scope, 'env> {
    pub(crate) jobs: &'env Jobs<'env>,
    pub(crate) scope: &'scope Scope<'scope, 'env>,
}

/// A monitor as the gate shows it: the whole command, where it runs, and whether it must leave
/// the sandbox.
fn watched(command: &str, dir: &std::path::Path, needs: Needs) -> String {
    let outside = match needs {
        Needs::Open => "  (outside the sandbox)",
        Needs::Sandbox => "",
    };
    format!("monitor: {command}  (in {}){outside}", dir.display())
}

impl<'scope, 'env> JobsTool<'scope, 'env> {
    /// Ask the gate: what a monitor's `command` (none for a schedule) may do, or the refusal. It
    /// is its subject: a deny on a command refuses a monitor that runs it (K5).
    fn ask(
        &self,
        command: &str,
        summary: &str,
        needs: Needs,
    ) -> Result<tools::shell::Scope, ToolError> {
        let effect = Effect {
            tool: spec::NAME,
            subject: command,
            summary,
            needs,
        };
        tools::shell::permit(self.jobs.kit.gate, effect).map_err(ToolError::Failed)
    }

    fn monitor(
        &self,
        command: &str,
        name: Option<String>,
        (timeout, needs): (Duration, Needs),
    ) -> Result<ToolOutput, ToolError> {
        let dir = self.jobs.kit.workspace.root();
        let scope = self.ask(command, &watched(command, dir, needs), needs)?;
        let timeout = match self.jobs.kit.session {
            true => timeout,
            false => timeout.min(Duration::from_millis(spec::EXEC_WATCH_MS)),
        };
        let stop = Arc::new(AtomicBool::new(false));
        // The deadline listed is the one kept.
        let until = Instant::now() + timeout;
        let (id, name) = {
            let mut state = self.jobs.lock();
            let running = state
                .jobs
                .iter()
                .filter(|job| matches!(job.what, What::Monitor { .. }))
                .count();
            if running >= MAX_MONITORS {
                return Err(ToolError::Failed(format!(
                    "{running} monitors run already: stop one first"
                )));
            }
            let (id, name) = new_job(&mut state, name);
            state.jobs.push(Job {
                id: id.clone(),
                name: name.clone(),
                what: What::Monitor {
                    command: command.to_string(),
                    until,
                    events: 0,
                },
                stop: Arc::clone(&stop),
                _known: self.jobs.known(&id, &stop),
            });
            (id, name)
        };
        self.jobs.show();
        let jobs = self.jobs;
        let kit = &jobs.kit;
        let (command, label, ours) = (command.to_string(), name.clone(), id.clone());
        let thread = std::thread::Builder::new().name("llx-monitor-job".into());
        let started = thread.spawn_scoped(self.scope, move || {
            let lines = |lines: Vec<String>, dropped: usize| {
                let mut text = lines.join("\n");
                if dropped > 0 {
                    let _ = write!(text, "\n… {dropped} more lines");
                }
                // A job no longer listed (`/new` let it go) says nothing more; a new one may
                // have its name, never its id.
                let mut state = jobs.lock();
                let Some(job) = state.jobs.iter_mut().find(|job| job.id == id) else {
                    return;
                };
                let What::Monitor { events, .. } = &mut job.what else {
                    return;
                };
                *events += 1;
                if *events >= MAX_EVENTS {
                    job.stop.store(true, Ordering::SeqCst);
                    let _ = write!(text, "\n(stopped after {MAX_EVENTS} batches: {RESTART})");
                }
                drop(state);
                kit.queue.post(crate::arrivals::monitor(&label, &text));
                jobs.show();
            };
            let stopped = || stop.load(Ordering::SeqCst);
            let hidden = (kit.hidden)();
            let ended = monitor::watch(
                &command,
                scope,
                kit.workspace,
                &hidden,
                until,
                &stopped,
                &lines,
            );
            // Keep it listed until its final arrival is posted: exec waits while it is listed.
            let listed = jobs.lock().jobs.iter().any(|job| job.id == id);
            let why = match ended {
                Ok(Ended::Exit(how)) => format!("its command ended ({how})"),
                Ok(Ended::Deadline) => {
                    "its deadline passed; start it again if it is still needed".into()
                }
                Ok(Ended::Stopped) => "it was stopped".into(),
                Err(why) => why,
            };
            if listed && !jobs.over.load(Ordering::SeqCst) {
                let ended = format!("The monitor ended: {why}.");
                kit.queue.post(crate::arrivals::monitor(&label, &ended));
            }
            jobs.lock().jobs.retain(|job| job.id != id);
            jobs.show();
        });
        // The OS refused a thread: the monitor is not listed, and the model is told.
        if let Err(why) = started {
            self.jobs.lock().jobs.retain(|job| job.id != ours);
            self.jobs.show();
            let why = format!("the monitor could not start (no thread: {why})");
            return Err(ToolError::Failed(why));
        }
        Ok(ToolOutput::from(format!(
            "Monitor `{name}` started: each line it prints comes to you as a `<monitor>` message. \
             Its end is reported within {} min. If idle, respond without a tool call and wait. \
             Do not poll the same work with `sleep`, `bash` or repeated `list` calls.",
            timeout.as_secs().div_ceil(60)
        )))
    }

    fn schedule(
        &self,
        prompt: &str,
        when: When,
        recurring: bool,
        name: Option<String>,
    ) -> Result<ToolOutput, ToolError> {
        if !self.jobs.kit.session {
            return Err(ToolError::Failed(
                "schedules need a session: this run ends with its task".into(),
            ));
        }
        let now = crate::clock::now();
        let (cron, next, said) = match when {
            When::Cron(cron) => {
                let next = cron
                    .next(now)
                    .ok_or_else(|| ToolError::Failed("it never runs within a year".into()))?;
                let said = format!(
                    "{} ({})",
                    cron.text(),
                    if recurring { "recurring" } else { "once" }
                );
                (recurring.then_some(cron), next, said)
            }
            // Past what the clock counts, it is never due, rather than due at once.
            When::In(seconds) => (
                None,
                now.saturating_add(seconds),
                format!("in {} min", seconds / 60),
            ),
        };
        self.ask("", &format!("schedule {said}: {prompt}"), Needs::Sandbox)?;
        let (id, name) = {
            let mut state = self.jobs.lock();
            let kept = state.jobs.len();
            if kept >= MAX_SCHEDULES {
                return Err(ToolError::Failed(format!(
                    "{kept} jobs are kept: stop one first"
                )));
            }
            let (id, name) = new_job(&mut state, name);
            let stop = Arc::new(AtomicBool::new(false));
            let known = self.jobs.known(&id, &stop);
            state.jobs.push(Job {
                id: id.clone(),
                name: name.clone(),
                what: What::Schedule {
                    prompt: prompt.to_string(),
                    cron,
                    next,
                    expires: now + WEEK,
                },
                stop,
                _known: known,
            });
            (id, name)
        };
        // The scheduler waits for the soonest it knew of: this one may be sooner.
        self.jobs.bell.ring();
        self.jobs.show();
        Ok(ToolOutput::from(format!(
            "Scheduled `{name}` ({id}), {said}: next at {}. It comes to you as a task of its own, \
             between tasks.",
            cron::clock(next)
        )))
    }

    fn list(&self) -> ToolOutput {
        let state = self.jobs.lock();
        if state.jobs.is_empty() {
            return ToolOutput::from("Nothing runs in the background.".to_string());
        }
        let rows: Vec<String> = state.jobs.iter().map(super::line).collect();
        ToolOutput::from(rows.join("\n"))
    }

    fn stop(&self, wanted: &str) -> Result<ToolOutput, ToolError> {
        let mut state = self.jobs.lock();
        let at = state
            .jobs
            .iter()
            .position(|job| job.id == wanted || job.name == wanted)
            .ok_or_else(|| ToolError::Failed(format!("there is no job `{wanted}`")))?;
        let job = &state.jobs[at];
        job.stop.store(true, Ordering::SeqCst);
        let name = job.name.clone();
        if matches!(job.what, What::Schedule { .. }) {
            state.jobs.remove(at);
        }
        drop(state);
        self.jobs.show();
        Ok(ToolOutput::from(format!("Stopped `{name}`.")))
    }
}

impl Tool for JobsTool<'_, '_> {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: spec::NAME.into(),
            description: spec::description(self.jobs.kit.session),
            input: Input::Json(spec::parameters(self.jobs.kit.session)),
        }
    }

    fn call(&self, arguments: &Value) -> Result<ToolOutput, ToolError> {
        match spec::action(arguments, self.jobs.kit.session).map_err(ToolError::Failed)? {
            Action::Monitor {
                command,
                name,
                timeout,
                needs,
            } => self.monitor(&command, name, (timeout, needs)),
            Action::Schedule {
                prompt,
                when,
                recurring,
                name,
            } => self.schedule(&prompt, when, recurring, name),
            Action::List => Ok(self.list()),
            Action::Stop { job } => self.stop(&job),
        }
    }
}

/// A new job's id and name (the name given, else its id).
fn new_job(state: &mut State, name: Option<String>) -> (String, String) {
    state.made += 1;
    let id = format!("job-{}", state.made);
    let name = name
        .filter(|name| state.jobs.iter().all(|job| job.name != *name))
        .unwrap_or_else(|| id.clone());
    (id, name)
}
