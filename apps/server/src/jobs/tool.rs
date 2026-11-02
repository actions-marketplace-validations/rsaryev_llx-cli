//! The `jobs` tool: the model starts a monitor or a schedule, lists them and stops them.

use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use std::thread::Scope;
use std::time::{Duration, Instant};

use contracts::gate::{Effect, Needs};
use contracts::tool::{Input, Tool, ToolError, ToolOutput, ToolSpec};
use serde_json::Value;

use super::spec::{self, Action, When};
use super::stop::{By, Stop};
use super::watched::Watched;
use super::{Job, Jobs, State, What, cron, outputs};
use crate::sync::lock;

/// Monitors running at once, at most.
const MAX_MONITORS: usize = 8;
/// Jobs of either kind kept, at most, for a schedule to be added.
const MAX_SCHEDULES: usize = 50;
/// How long a recurring schedule lives.
const WEEK: u64 = 7 * 86_400;

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
        (every, timeout, needs): (Duration, Duration, Needs),
    ) -> Result<ToolOutput, ToolError> {
        let dir = self.jobs.kit.workspace.root();
        // Refused before the person is asked about it; looked at again where it is listed (R7).
        room_for_a_monitor(&self.jobs.lock())?;
        let scope = self.ask(command, &watched(command, dir, needs), needs)?;
        let timeout = match self.jobs.kit.session {
            true => timeout,
            false => timeout.min(Duration::from_millis(spec::EXEC_WATCH_MS)),
        };
        let stop = Arc::new(Stop::default());
        // The deadline listed is the one kept.
        let until = Instant::now() + timeout;
        let checks = Arc::new(AtomicUsize::new(0));
        let last: Arc<Mutex<String>> = Arc::default();
        let (id, name) = {
            let mut state = self.jobs.lock();
            room_for_a_monitor(&state)?;
            let (id, name) = super::new_job(&mut state, name);
            let key = crate::interrupt::job_key(&self.jobs.owner, &id);
            let shown = Arc::clone(&last);
            state.jobs.push(Job {
                id: id.clone(),
                name: name.clone(),
                what: What::Monitor {
                    command: command.to_string(),
                    until,
                    checks: Arc::clone(&checks),
                    _output: outputs::register(&key, move || lock(&shown).clone()),
                },
                stop: Arc::clone(&stop),
                _known: self.jobs.known(&id, &stop),
            });
            (id, name)
        };
        self.jobs.show();
        let jobs = self.jobs;
        let job = Watched {
            id: id.clone(),
            name: name.clone(),
            command: command.to_string(),
            scope,
            every,
            until,
            stop,
            checks,
            last,
        };
        let thread = std::thread::Builder::new().name("llx-monitor-job".into());
        // The OS refused a thread: the monitor is not listed, and the model is told.
        if let Err(why) = thread.spawn_scoped(self.scope, move || job.run(jobs)) {
            self.jobs.lock().jobs.retain(|job| job.id != id);
            self.jobs.show();
            let why = format!("the monitor could not start (no thread: {why})");
            return Err(ToolError::Failed(why));
        }
        Ok(ToolOutput::from(format!(
            "Monitor `{name}` ({id}) started: `{command}` every {} s, for at most {} min. You are \
             told once, when it exits 0 or its time is up: if there is nothing else to do, respond \
             without a tool call and wait. Do not check it yourself with `sleep`, `bash` or \
             `list`.",
            every.as_secs(),
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
        // As a monitor's: refused before the person is asked, and looked at again (R7).
        room_for_a_schedule(&self.jobs.lock())?;
        self.ask("", &format!("schedule {said}: {prompt}"), Needs::Sandbox)?;
        let (id, name) = {
            let mut state = self.jobs.lock();
            room_for_a_schedule(&state)?;
            let (id, name) = super::new_job(&mut state, name);
            let stop = Arc::new(Stop::default());
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
        job.stop.ask(By::Run);
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
                every,
                timeout,
                needs,
            } => self.monitor(&command, name, (every, timeout, needs)),
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

/// Whether one more monitor may run: no more than [`MAX_MONITORS`] at once.
fn room_for_a_monitor(state: &State) -> Result<(), ToolError> {
    let running = (state.jobs.iter())
        .filter(|job| matches!(job.what, What::Monitor { .. }))
        .count();
    match running < MAX_MONITORS {
        true => Ok(()),
        false => Err(ToolError::Failed(format!(
            "{running} monitors run already: stop one first"
        ))),
    }
}

/// Whether one more schedule may be kept: no more than [`MAX_SCHEDULES`] jobs of either kind.
fn room_for_a_schedule(state: &State) -> Result<(), ToolError> {
    let kept = state.jobs.len();
    match kept < MAX_SCHEDULES {
        true => Ok(()),
        false => Err(ToolError::Failed(format!(
            "{kept} jobs are kept: stop one first"
        ))),
    }
}
