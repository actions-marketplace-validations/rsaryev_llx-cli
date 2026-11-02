//! [`Rig`]: the built agent of a lane and what a task needs around it, built by [`drive`] from
//! what `assemble` and `equip` made, and the work run with it.

use std::sync::Mutex;
use std::sync::atomic::AtomicU64;

use agent::agent::Agent;
use agent::conversation::Conversation;
use contracts::advice::Advisor;
use contracts::history::Session;
use contracts::tool::Tool;

use super::{Base, Lane, Seat, Shared};
use crate::agents::fork::Recording;
use crate::agents::{AgentsTool, Registry};
use crate::brake::Brake;
use crate::choice::Choice;
use crate::interrupt::Interrupt;
use crate::jobs::{Jobs, JobsTool};
use crate::live::LiveChat;
use crate::mode::{self, Mode, ModeCell};
use crate::output::Output;
use crate::providers::Provider;
use crate::server::Ui;
use crate::sync::lock;
use protocol::Event;

/// The built agent and what a task needs around it; one per lane, for every task it runs.
pub(crate) struct Rig<'a> {
    pub agent: Agent<'a>,
    /// What the session's lanes share: the options, the workspace, the session store, the modes
    /// (the command line's rules joined to each).
    pub shared: &'a Shared,
    pub output: &'a Output,
    pub brake: &'a Brake<'a>,
    /// Whether the person stopped the task running.
    pub interrupt: &'a Interrupt,
    /// The agents the session started: let go with it, and what they spend is the task's.
    pub agents: &'a Registry,
    /// The monitors and schedules running in the background.
    pub jobs: &'a Jobs<'a>,
    /// The model's context window, for the agents (set at each task).
    pub window: &'a AtomicU64,
    /// Each task of the conversation, oldest first, and where the conversation stood before it:
    /// what `/rewind` goes back to.
    pub checkpoints: Vec<crate::checkpoints::Task>,
    /// What the session's tasks spent, and when it began: `/usage`.
    pub spent: contracts::chat::Spend,
    pub begun: std::time::Instant,
    pub session: Session,
    /// The conversation so far: the next task goes on from it.
    pub conversation: Option<Conversation>,
    /// The mode now: the gate and the person read it at each question.
    pub mode: &'a ModeCell,
    /// The mode the model last heard of.
    pub told: Mode,
    /// The interface the session is shown in, at a terminal.
    pub ui: Option<Ui>,
    /// The pictures the person attached to the next task.
    pub attached: Vec<std::path::PathBuf>,
    /// What the agent thinks with now.
    pub choice: Choice,
    pub live: Option<&'a LiveChat>,
    /// What commands never see, read at each command.
    pub hidden: &'a Mutex<Vec<String>>,
    /// What is known of the session beside its tasks, made in the background, not saved yet.
    pub meta: crate::meta::Pending,
    /// The decision model's advice, told the levels it may pick at each task.
    pub advice: Option<&'a crate::advice::Advice<'a>>,
}

/// The agent built from `base` and `lane` with `seat`'s session, and `work` run with it. The
/// agents' threads end with the session at the latest: each task waits for its own. The guard
/// ends the jobs and stops the agents even when the session unwinds from a panic, or the scope
/// would wait for their threads for ever.
pub(super) fn drive(
    base: &Base,
    lane: &Lane,
    seat: Seat,
    work: impl FnOnce(&mut Rig) -> i32,
) -> i32 {
    let Seat {
        context,
        session,
        ui,
        schema,
    } = seat;
    let shared = base.shared;
    std::thread::scope(|scope| {
        // Inside the scope: it is dropped while unwinding, before the scope waits for threads.
        let _ending = Ending {
            jobs: lane.jobs,
            registry: lane.registry,
        };
        let agents = AgentsTool {
            kit: lane.kit,
            registry: lane.registry,
            scope,
        };
        lane.registry.adopt(&session.id, base.output);
        let jobs_tool = JobsTool {
            jobs: lane.jobs,
            scope,
        };
        if shared.asks {
            let thread = std::thread::Builder::new().name("llx-scheduler".into());
            if let Err(why) = thread.spawn_scoped(scope, || lane.jobs.tick()) {
                let said =
                    format!("llx: schedules will not run in this session (no thread: {why})");
                base.output.note(&said);
            }
        }
        let advisor = (lane.kit.advice).map(|advice| advice.advisor(crate::advice::Role::Main));
        // The main agent's exchanges, kept for the agents it forks.
        let recording = Recording {
            inner: base.brake,
            forks: &lane.kit.forks,
        };
        lane.tools.with(|own| {
            let mut tools: Vec<&dyn Tool> = own.to_vec();
            if shared.asks {
                tools.push(base.ask);
            }
            tools.push(&agents);
            tools.push(&jobs_tool);
            // Once per lane: `drive` runs once per `assemble`, so the cell is empty here.
            let _ = lane
                .kit
                .main
                .set(tools.iter().map(|tool| tool.spec()).collect());
            let agent = Agent {
                chat: &recording,
                tools: &tools,
                context: &context,
                max_turns: lane.kit.max_turns,
                observer: base.output,
                schema: schema.as_ref(),
                inbox: Some(lane.inbox),
                window: 0,
                strategy: lane.kit.strategy,
                advisor: advisor.as_ref().map(|advisor| advisor as &dyn Advisor),
            };
            let mut rig = Rig {
                agent,
                shared,
                output: base.output,
                brake: base.brake,
                interrupt: base.interrupt,
                agents: lane.registry,
                jobs: lane.jobs,
                window: lane.kit.window,
                checkpoints: vec![],
                spent: contracts::chat::Spend::default(),
                begun: std::time::Instant::now(),
                session,
                conversation: None,
                mode: base.mode,
                told: shared.started.clone(),
                ui,
                attached: vec![],
                choice: shared.choice.clone(),
                live: base.live,
                hidden: lane.hidden,
                meta: crate::meta::Pending::default(),
                advice: lane.kit.advice,
            };
            let code = work(&mut rig);
            rig.agents.settle();
            code
        })
    })
}

/// Ends the session's background work when it is dropped.
struct Ending<'a> {
    jobs: &'a Jobs<'a>,
    registry: &'a Registry,
}

impl Drop for Ending<'_> {
    fn drop(&mut self) {
        self.jobs.close();
        self.registry.stop_all();
    }
}

impl Rig<'_> {
    /// Keep the variables of `provider`'s key from commands too, from the next command on: a
    /// provider chosen after the session began. Those of the one before stay hidden.
    pub(crate) fn hide_for(&self, provider: &Provider) {
        let mut hidden = lock(self.hidden);
        for name in provider.hidden() {
            if !hidden.contains(&name) {
                hidden.push(name);
            }
        }
    }

    /// Work in `mode` from the next question on.
    pub(crate) fn switch(&mut self, mode: Mode) {
        self.mode.set(mode);
        if let Some(ui) = &self.ui {
            ui.send(Event::Mode(self.mode.get().name().to_string()));
        }
    }

    /// The conversation the next task goes on from, told first when the mode changed since the
    /// model last heard of it (in the context, or in an earlier note). Nothing is said when it
    /// did not change, so the conversation's prefix stays the same for the prompt cache.
    pub(crate) fn told_of_mode(&mut self) -> Option<Conversation> {
        let conversation = self.conversation.take();
        let mode = self.mode.get();
        if mode == self.told {
            return conversation;
        }
        self.told = mode.clone();
        let note = mode::notice(&mode, self.shared.terminal, self.shared.asks);
        Some(self.agent.tell(conversation, &note))
    }
}
