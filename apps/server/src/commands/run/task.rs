//! One task of a session, on its [`Rig`]: run, reported, recorded.

use std::time::{Duration, Instant};

use contracts::history::Turn;
use contracts::tool::Image;
use serde_json::Value;

use super::Ended;
use crate::attach;
use crate::brake::Limits;
use crate::compose::Rig;
use crate::exit;
use crate::output::{Start, Summary};

impl Rig<'_> {
    /// Run one task in the session: report it, record it, and return its exit code.
    pub(crate) fn task(&mut self, goal: &str) -> i32 {
        let started = Instant::now();
        let workspace = self.shared.workspace.root().display().to_string();
        self.output.started(&Start {
            goal,
            session: &self.session.id,
            model: self.choice.model(),
            workspace: &workspace,
        });
        let attached = self.pictures();
        crate::meta::title::start(self, goal);
        let limits = Limits {
            time: Duration::from_secs(self.shared.options.timeout),
            budget: self.shared.options.budget,
        };
        self.brake.arm(limits);
        self.interrupt.begin();
        self.jobs.queue().begin();
        let at = self.conversation.as_ref().map_or(0, |c| c.messages().len());
        self.checkpoints.push(crate::checkpoints::Task {
            goal: goal.to_string(),
            at,
        });
        if let Some(advice) = self.advice {
            advice.offer(&self.choice);
        }
        // A level the person chose stands over one picked for the conversation under `auto`.
        if self.choice.level == crate::choice::Level::Fixed
            && let Some(conversation) = &mut self.conversation
        {
            conversation.set_effort(None);
        }
        // The model may have changed since the last task: its window decides when to compact.
        self.agent.window = self.choice.selection.model().context;
        self.window
            .store(self.agent.window, std::sync::atomic::Ordering::SeqCst);
        let before = self.told_of_mode();
        let mut run = self.agent.run(before, goal, &attached);
        self.agents.settle();
        self.jobs.end();
        self.jobs.queue().end();
        run.spend.absorb(&self.agents.take_spent());
        self.spent.absorb(&run.spend);
        self.context_left(run.used);
        let halt = self.brake.halted();
        self.interrupt.end();
        self.conversation = Some(run.conversation);
        let ended = Ended::of(&run.status, run.data.as_ref(), halt, limits);
        self.record(goal, &run.answer, ended.status);
        // `-o` first, so `exit_code` in the report is the code the process ends with.
        let (code, error) = match self.write_last(&run.answer, run.data.as_ref()) {
            Ok(()) => (ended.code, ended.error),
            Err(written) => {
                let error = match ended.error {
                    Some(error) => format!("{error}; {written}"),
                    None => written,
                };
                (exit::ENVIRONMENT, Some(error))
            }
        };
        let summary = Summary {
            status: ended.status,
            code,
            answer: &run.answer,
            shown: run.shown,
            data: run.data.as_ref(),
            error: error.as_deref(),
            agent: &run.spend,
            millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            session: &self.session.id,
        };
        self.output.completed(&summary);
        if self.output.closed() { exit::OK } else { code }
    }

    /// The pictures attached to this task, read; one that cannot be is left out, and said.
    fn pictures(&mut self) -> Vec<(String, Image)> {
        let mut out = vec![];
        for path in std::mem::take(&mut self.attached) {
            match attach::read(&path) {
                Ok(picture) => out.push((picture.name, picture.image)),
                Err(why) => self.output.note(&format!("llx: not attached: {why}\n")),
            }
        }
        out
    }

    /// `-o FILE`: the final answer, or with `--schema` the data. With `--schema` and no data that
    /// matched, the file is left empty: prose where data is expected would break the reader.
    fn write_last(&self, answer: &str, data: Option<&Result<Value, String>>) -> Result<(), String> {
        let Some(path) = &self.shared.options.output_last_message else {
            return Ok(());
        };
        let last = match (data, self.agent.schema.is_some()) {
            (Some(Ok(data)), _) => format!("{data}\n"),
            (_, true) => String::new(),
            (_, false) => format!("{}\n", answer.trim()),
        };
        std::fs::write(path, last).map_err(|error| format!("-o {}: {error}", path.display()))
    }

    /// How much of the model's context window is left, for the interface: `used` tokens of it.
    fn context_left(&self, used: u64) {
        let ceiling = self.agent.strategy.ceiling(self.agent.window);
        if let (Some(ui), true) = (&self.ui, self.agent.window > 0) {
            let left = 100u64.saturating_sub(used * 100 / ceiling.max(1));
            ui.send(protocol::Event::Context(
                u8::try_from(left).unwrap_or(u8::MAX),
            ));
        }
    }

    /// Add the task to the session, and save it.
    fn record(&mut self, goal: &str, answer: &str, status: &str) {
        let turn = Turn {
            goal: goal.to_string(),
            answer: answer.to_string(),
            status: status.to_string(),
            at: crate::clock::now(),
            steps: self.output.take_steps(),
        };
        if let Err(error) = self.meta.turn(&mut self.session, turn) {
            self.output
                .note(&format!("llx: the session was not saved: {error}\n"));
        }
    }
}
