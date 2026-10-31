//! One task of an agent, on its own thread: its conversation goes on (or opens with a brief), its
//! answer is posted to the main agent, and the task is recorded in its session.

use std::fmt::Write as _;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use agent::agent::{Agent, Run, Status};
use contracts::advice::Advisor;
use contracts::chat::{Chat, ChatError, Completion, Request};
use contracts::history::Turn;

use super::Registry;
use super::inbox::AgentInbox;
use super::kit::{Kit, Tools};
use super::watch::{Labeled, Watch};
use crate::brake::{Halt, Limits};
use crate::mode;
use protocol::Event;

/// What an agent is told before its first task: who it answers to, and how.
fn brief(name: &str) -> String {
    format!(
        "You are `{name}`, an agent another agent started for one task. Your answer goes back to \
         that agent, not to the person, and it is all it gets: make it complete on its own. Say \
         what you found or changed (paths with line numbers), what you checked and how, and what \
         is left. Nobody answers questions here: decide on your best assumption and state it. \
         Do this task only."
    )
}

/// What a fork is told before its first task: the conversation above is its own start.
fn fork_brief(name: &str) -> String {
    format!(
        "You are `{name}`, an agent this conversation's agent started for one task: the \
         conversation above is its work so far, and yours to build on; read nothing again that \
         it already shows. From here on you work on the task below alone. Your answer goes back \
         to that agent, not to the person, and it is all it gets: say what you found or changed \
         (paths with line numbers), what you checked and how, and what is left; leave out what \
         the conversation above already says. Nobody answers questions here: decide on your \
         best assumption and state it. Do this task only."
    )
}

/// The chat model, stopped when this agent is.
struct Stoppable<'a> {
    inner: &'a dyn Chat,
    stop: &'a (dyn Fn() -> bool + Sync),
}

impl Chat for Stoppable<'_> {
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        if (self.stop)() {
            return Err(ChatError::Unavailable("the agent was stopped".into()));
        }
        self.inner.complete(request)
    }
}

/// Give the agent with session `id` the task `message`, and post its answer.
pub(super) fn work(kit: &Kit, registry: &Registry, id: &str, message: &str) {
    let taken = {
        let mut state = registry.lock();
        state
            .children
            .iter_mut()
            .find(|c| c.session.id == id)
            .map(|child| {
                (
                    child.session.name.clone(),
                    child.conversation.take(),
                    child.session.turns.is_empty(),
                    child.told.clone(),
                    Arc::clone(&child.stop),
                    Arc::clone(&child.letters),
                )
            })
    };
    let Some((name, conversation, first, told, flag, letters)) = taken else {
        return;
    };
    let stop = || (kit.stopped)() || flag.load(Ordering::SeqCst);
    let gate = Labeled {
        inner: kit.gate,
        label: name.clone(),
    };
    let tools = Tools::new(kit, &gate, &stop);
    let watch = Watch {
        output: kit.output,
        id,
        name: &name,
    };
    let inbox = AgentInbox {
        letters: &letters,
        watch: &watch,
    };
    let chat = Stoppable {
        inner: kit.chat,
        stop: &stop,
    };
    let now = kit.mode.get();
    watch.show(Event::TaskStarted(message.to_string()));
    let started = Instant::now();
    let advisor = (kit.advice).map(|advice| advice.advisor(crate::advice::Role::Agent));
    let run = tools.as_main(|tools| {
        let agent = Agent {
            chat: &chat,
            tools,
            context: kit.context,
            max_turns: kit.max_turns,
            window: kit.window.load(Ordering::SeqCst),
            strategy: kit.strategy,
            advisor: advisor.as_ref().map(|advisor| advisor as &dyn Advisor),
            observer: &watch,
            schema: None,
            inbox: Some(&inbox),
        };
        // The brief first, and the mode when it is not the one the context states.
        let mut said = vec![];
        match (&conversation, first) {
            (None, _) => said.push(brief(&name)),
            (Some(_), true) => said.push(fork_brief(&name)),
            (Some(_), false) => {}
        }
        if now != told {
            said.push(mode::notice(&now, kit.terminal, false));
        }
        let conversation = match said.is_empty() {
            true => conversation,
            false => Some(agent.tell(conversation, &said.join("\n\n"))),
        };
        agent.run(conversation, message, &[])
    });
    let halt = stop().then_some(Halt::Interrupted);
    // An agent's task has no limits of its own.
    let status = crate::commands::run::Ended::of(&run.status, None, halt, Limits::default()).status;
    let millis = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    watch.show(Event::Answer(run.answer.clone()));
    watch.show(Event::TaskEnded {
        status: status.to_string(),
        millis,
        turns: run.spend.calls,
        cost: run.spend.cost,
    });
    keep(kit, registry, id, message, run, (status, halt), now);
}

/// Put the agent's conversation back, record its task, count its spend and post its answer: the
/// task ended as `status` says, and `halt` says whether it was stopped. The registry is locked
/// to change it only: the session is saved and the answer posted after (R6).
fn keep(
    kit: &Kit,
    registry: &Registry,
    id: &str,
    message: &str,
    run: Run,
    (status, halt): (&str, Option<Halt>),
    told: mode::Mode,
) {
    let recorded = {
        let mut state = registry.lock();
        state.spent.absorb(&run.spend);
        state
            .children
            .iter_mut()
            .find(|c| c.session.id == id)
            .map(|child| {
                child.told = told;
                child.status = status.to_string();
                if child.open {
                    child.conversation = Some(run.conversation);
                }
                child.session.record(Turn {
                    goal: message.to_string(),
                    answer: run.answer.clone(),
                    status: status.to_string(),
                    at: crate::clock::now(),
                    steps: vec![],
                });
                // Kept for the work done after the lock, with what the main agent sent after the
                // agent's last look at its letters.
                (child.session.clone(), child.letters.take(), child.open)
            })
    };
    let Some((session, unread, open)) = recorded else {
        registry.changed.notify_all();
        return;
    };
    let saved = kit.history.save(&session);
    let mut answer = match (&run.status, halt) {
        (Status::Exhausted, _) => "It ran out of turns before an answer: `send` it a message to \
                                   have it answer with what it has."
            .to_string(),
        (Status::Error(why), None) => {
            format!("It stopped: {why}. Its conversation is kept: `send` to go on with it.")
        }
        (Status::Done | Status::Error(_), Some(_)) if run.answer.trim().is_empty() => {
            "It was stopped before it answered. Its conversation is kept: `send` to go on with \
             it, or leave it."
                .to_string()
        }
        (Status::Done | Status::Error(_), _) => run.answer.trim().to_string(),
    };
    if !unread.is_empty() {
        let _ = write!(
            answer,
            "\n\n(it ended before reading: {}; `send` again to have it act on them)",
            unread.join(" | ")
        );
    }
    if let Err(error) = saved {
        let _ = write!(answer, "\n\n(its session was not saved: {error})");
    }
    if open {
        let came = crate::arrivals::report(&session.name, id, status, &answer);
        registry.queue.post(came);
    }
    // Working ends after the answer is posted: a wait that sees no agent working finds it.
    if let Some(child) = registry
        .lock()
        .children
        .iter_mut()
        .find(|c| c.session.id == id)
    {
        child.working = false;
    }
    registry.changed.notify_all();
}
