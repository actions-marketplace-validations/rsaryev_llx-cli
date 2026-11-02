//! The composition root: the one place that builds the packages and hands them to the agent
//! through its ports. [`compose`] builds the chat model; [`assemble`] builds the rest around any
//! model: the brake, the workspace, the mode's gate, the person, and the tools: `bash`,
//! `apply_patch`, `grep`, `ask` in a session, `agents`, `jobs`. What it starts (threads, monitors) is closed when `assemble` returns,
//! before `main` exits (which would skip their `Drop`).

mod lane;
mod rig;
mod setup;

#[cfg(any(test, feature = "testing"))]
pub(crate) use lane::lane_with;
pub(crate) use lane::{Models, compose, lane};
pub(crate) use rig::Rig;
pub(crate) use setup::{Seat, Setup, Shared};

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, OnceLock};

use contracts::gate::Gate;
use contracts::person::{Person, Question, Reply};
use provider_kit::App;
use tools::ask::AskTool;

use crate::agents::Registry;
use crate::agents::inbox::MainInbox;
use crate::agents::kit::{Kit, Tools};
use crate::brake::Brake;
use crate::gate::{Asker, Stoppable, TerminalGate, Tty};
use crate::interrupt::Interrupt;
use crate::jobs::{self, Jobs};
use crate::live::LiveChat;
use crate::mode::{ModeCell, ModeGate};
use crate::output::Output;
use crate::server::asking::{UiGate, UiPerson};
use crate::sync::lock;

/// Who calls the providers: attribution, the sign-in pages, the folder sign-ins are saved in.
pub(crate) const APP: App = App {
    name: "llx",
    slug: "llx",
    version: env!("CARGO_PKG_VERSION"),
    site: "https://llx.sh",
};

/// Build a lane's agent from `seat` over `shared`, around `models`, reporting on `output` and
/// stopped by `interrupt`, and run `work` with it: here the brake, the mode's gate and the
/// person; [`equip`] the rest.
pub(crate) fn assemble(
    shared: &Shared,
    seat: Seat,
    models: Models,
    output: &Output,
    interrupt: &Interrupt,
    work: impl FnOnce(&mut Rig) -> i32,
) -> i32 {
    let closed = || output.closed();
    let stopped = || interrupt.stopped();
    let brake = Brake::new(models.chat, &closed, &stopped);
    // The tools' stop: the person's, or the time limit's, so a command running past it is killed
    // as a stop kills it. The gate and the person keep `stopped`: waiting on them is not the
    // task's time (`waited`).
    let limited = || stopped() || brake.due();
    let mode = ModeCell::new(shared.started.clone());
    let waited = |took| brake.extend(took);
    let asker: Option<Box<dyn Asker>> = match &seat.ui {
        Some(ui) => Some(Box::new(UiGate {
            ui: ui.clone(),
            stopped: &stopped,
            asking: &shared.asking,
        })),
        None => (shared.terminal).then(|| Box::new(TerminalGate(Tty)) as Box<dyn Asker>),
    };
    let approver = ModeGate::new(&mode, asker, &waited);
    let gate = Stoppable {
        inner: &approver,
        stopped: &stopped,
    };
    let person: Box<dyn Person> = match &seat.ui {
        Some(ui) => Box::new(UiPerson {
            ui: ui.clone(),
            stopped: &stopped,
            waited: &waited,
            asking: &shared.asking,
        }),
        None => Box::new(Nobody),
    };
    let ask = AskTool {
        person: person.as_ref(),
    };
    let base = Base {
        shared,
        output,
        interrupt,
        brake: &brake,
        mode: &mode,
        gate: &gate,
        ask: &ask,
        stopped: &limited,
        live: models.live,
        adviser: models.adviser,
        searcher: models.searcher,
        namer: models.namer,
    };
    equip(&base, seat, work)
}

/// The tools around `base`, the agents and the jobs, the lane's queue and its inbox; then the
/// rig is built and `work` run with it ([`rig::drive`]).
fn equip(base: &Base, seat: Seat, work: impl FnOnce(&mut Rig) -> i32) -> i32 {
    let (shared, output) = (base.shared, base.output);
    let Shared {
        options,
        workspace,
        history,
        fresh,
        terminal,
        asks,
        ..
    } = shared;
    // What commands never see: the variables of the key the models are called with, and of a
    // provider chosen later with `/login` (hiding is no barrier anyway).
    let hidden_names = Mutex::new(shared.choice.provider.hidden());
    let hidden = || lock(&hidden_names).clone();
    // The lane's queue: what reaches its run from outside its calls.
    let lane_queue = Arc::new(crate::arrivals::queue(output.ui().cloned()));
    let window = AtomicU64::new(shared.choice.selection.model().context);
    // Each participant keeps its own conversation; one pipeline prepares them all, keeping the
    // instruction files read as long as it can.
    let strategy = agent::history::Pipeline::keeping(Box::new(crate::instructions::reads_one));
    // The decision model advises each task's edges (C9).
    let charge = |cost| base.brake.charge(cost);
    // The levels it may pick are set as each task starts (`Rig::task`), from the choice then.
    let advice = (base.adviser).map(|decider| crate::advice::Advice::new(decider, &charge));
    // Agents build tools of their own from these, on a gate that names them; `agents` (one
    // level), `ask` (nobody answers an agent) and `jobs` are offered to them but refused.
    let kit = Kit {
        chat: base.brake,
        workspace,
        gate: base.gate,
        hidden: &hidden,
        stopped: base.stopped,
        context: fresh,
        max_turns: options.max_turns.unwrap_or(usize::MAX),
        window: &window,
        strategy: &strategy,
        output,
        history,
        mode: base.mode,
        started: shared.started.clone(),
        terminal: *terminal,
        main: OnceLock::new(),
        forks: crate::agents::fork::Forks::default(),
        advice: advice.as_ref(),
        searcher: base.searcher,
        charge: &charge,
    };
    // The main session's tools are an agent's, on the session's own gate and stop.
    let main_tools = Tools::new(&kit, base.gate, base.stopped);
    let registry = Registry::new(Arc::clone(&lane_queue));
    let jobs_kit = jobs::Kit {
        workspace,
        gate: base.gate,
        hidden: &hidden,
        output,
        session: *asks,
        queue: &lane_queue,
    };
    let jobs = Jobs::new(&seat.session.id, jobs_kit);
    let inbox = MainInbox {
        registry: &registry,
        jobs: &jobs,
        output,
        stopped: base.stopped,
    };
    let lane = Lane {
        kit: &kit,
        tools: &main_tools,
        registry: &registry,
        jobs: &jobs,
        inbox: &inbox,
        hidden: &hidden_names,
    };
    rig::drive(base, &lane, seat, work)
}

/// What [`assemble`] made, for the rest of the lane to borrow.
struct Base<'a> {
    shared: &'a Shared,
    output: &'a Output,
    interrupt: &'a Interrupt,
    brake: &'a Brake<'a>,
    mode: &'a ModeCell,
    gate: &'a dyn Gate,
    ask: &'a AskTool<'a>,
    stopped: &'a (dyn Fn() -> bool + Sync),
    live: Option<&'a LiveChat>,
    adviser: Option<&'a dyn contracts::model::Decider>,
    searcher: Option<&'a dyn contracts::model::Decider>,
    namer: Option<&'a crate::meta::title::Namer>,
}

/// What [`equip`] made around the base: the tools, the agents, the jobs and the inbox.
struct Lane<'a> {
    kit: &'a Kit<'a>,
    tools: &'a Tools<'a>,
    registry: &'a Registry,
    jobs: &'a Jobs<'a>,
    inbox: &'a MainInbox<'a>,
    hidden: &'a Mutex<Vec<String>>,
}

/// Who answers with no interface (`exec`, where `ask` is not offered anyway): nobody.
struct Nobody;

impl Person for Nobody {
    fn ask(&self, _: &[Question]) -> Reply {
        Reply::Nobody("nobody is at a terminal to answer".into())
    }
}

#[cfg(test)]
#[path = "compose_tests.rs"]
mod tests;
