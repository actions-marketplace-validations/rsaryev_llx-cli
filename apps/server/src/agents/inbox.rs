//! What reaches a run between its turns. The main agent's: what the person sent while it worked
//! (first: the person comes before what they started), then the lane's queue, oldest first. An
//! agent's own: what the main agent sent it while it worked.

use std::sync::Mutex;

use contracts::inbox::Inbox;

use super::watch::Watch;
use super::{POLL, Registry};
use crate::output::Output;
use protocol::Event;

/// The main agent's inbox: the person's words, then the lane's queue, which holds whatever else
/// came (agents' answers, monitors, ...) and knows no source.
pub(crate) struct MainInbox<'a> {
    pub registry: &'a Registry,
    pub jobs: &'a crate::jobs::Jobs<'a>,
    pub output: &'a Output,
    pub stopped: &'a (dyn Fn() -> bool + Sync),
}

impl Inbox for MainInbox<'_> {
    fn take(&self) -> Vec<String> {
        // What the person sent, taken from the interface (which shows it as theirs now).
        let mut came = self.output.ui().map(|ui| ui.steered()).unwrap_or_default();
        came.extend(
            self.registry
                .queue
                .take_joining()
                .iter()
                .map(queue::Entry::render),
        );
        came
    }

    fn expecting(&self) -> bool {
        self.registry.working() > 0 || self.jobs.expecting()
    }

    fn wait(&self) -> Vec<String> {
        let working = self.registry.working();
        let said = match working {
            0 => "✻ Waiting for the jobs in the background to report\n".to_string(),
            n => format!(
                "✻ Waiting for {} to finish\n",
                crate::output::count(n, "background agent")
            ),
        };
        let last = std::mem::replace(&mut *crate::sync::lock(&self.registry.said), said.clone());
        if last != said {
            self.output.note(&said);
        }
        loop {
            // Asked before taking: an agent posts its answer as it stops working, so once none
            // is expected, every answer is already in the queue.
            let expecting = self.expecting();
            let came = self.take();
            if !came.is_empty() || !expecting || (self.stopped)() {
                return came;
            }
            self.registry.queue.wait(POLL);
        }
    }
}

/// What the main agent sends an agent while it works.
#[derive(Default)]
pub(crate) struct Letters(Mutex<Vec<String>>);

impl Letters {
    pub(crate) fn post(&self, message: &str) {
        crate::sync::lock(&self.0).push(message.to_string());
    }

    pub(super) fn take(&self) -> Vec<String> {
        std::mem::take(&mut *crate::sync::lock(&self.0))
    }
}

/// An agent's inbox: its letters, shown in its view as they are read.
pub(crate) struct AgentInbox<'a> {
    pub letters: &'a Letters,
    pub watch: &'a Watch<'a>,
}

impl Inbox for AgentInbox<'_> {
    fn take(&self) -> Vec<String> {
        let letters = self.letters.take();
        for letter in &letters {
            self.watch.show(Event::Note(letter.clone()));
        }
        letters
            .into_iter()
            .map(|letter| format!("The agent that started you says: {letter}"))
            .collect()
    }

    fn expecting(&self) -> bool {
        false
    }

    fn wait(&self) -> Vec<String> {
        vec![]
    }
}

#[cfg(test)]
#[path = "inbox_tests.rs"]
mod tests;
