//! One queue for everything that reaches a session's run from outside its own calls: an agent's
//! answer, a monitor's lines, a schedule's prompt, and whatever comes later. The queue knows no
//! source. Each entry carries its own policy, the one table every source answers: whether a
//! running task takes it at its next step, and what becomes of it while no task runs. The reader
//! (the agent's inbox) knows only entries.
//!
//! Nothing leaks: the queue is bounded (the oldest go first, and the reader is told how many),
//! an entry's body is data inside its tag and cannot close it, and a source's entries can be
//! forgotten with the source.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

/// What becomes of an entry that comes while no task runs, or that a task ended without reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Idle {
    /// It starts a task of its own (a monitor's lines); with no one to open it, it waits for the
    /// next task and joins it at its first step.
    Opens,
    /// It was for the task that is over (an agent's late answer).
    Drops,
}

/// What a source says of its entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// A running task takes it at its next step.
    pub joins: bool,
    pub idle: Idle,
}

/// One thing that came, as its source made it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub policy: Policy,
    /// What it is, and the tag it is read in (`agent_result`, `monitor`); empty for one read as
    /// it is (a schedule's prompt, a line of the queue's own).
    pub tag: String,
    /// The tag's attributes, in order (`job="j2"`).
    pub attrs: Vec<(String, String)>,
    pub body: String,
    /// Its source (an agent's session, a job), to take or forget its entries by.
    pub key: Option<String>,
    /// A line before the tag: how to read it.
    pub preface: Option<String>,
}

impl Entry {
    pub fn new(policy: Policy, tag: &str, body: &str) -> Self {
        Self {
            policy,
            tag: tag.into(),
            attrs: vec![],
            body: body.into(),
            key: None,
            preface: None,
        }
    }

    #[must_use]
    pub fn keyed(mut self, key: &str) -> Self {
        self.key = Some(key.into());
        self
    }

    #[must_use]
    pub fn attr(mut self, name: &str, value: &str) -> Self {
        self.attrs.push((name.into(), value.into()));
        self
    }

    #[must_use]
    pub fn preface(mut self, text: &str) -> Self {
        self.preface = Some(text.into());
        self
    }

    /// As the model reads it: the body inside its tag, as data that cannot end the tag early.
    pub fn render(&self) -> String {
        if self.tag.is_empty() {
            return self.body.clone();
        }
        let attrs = self
            .attrs
            .iter()
            .fold(String::new(), |mut attrs, (name, value)| {
                let value = value.replace('"', "&quot;");
                // Writing to a String cannot fail.
                let _ = write!(attrs, " {name}=\"{value}\"");
                attrs
            });
        let close = format!("</{}", self.tag);
        let body = self.body.replace(&close, &format!("<\\/{}", self.tag));
        let tagged = format!("<{}{attrs}>\n{body}\n</{}>", self.tag, self.tag);
        match &self.preface {
            Some(preface) => format!("{preface}\n{tagged}"),
            None => tagged,
        }
    }
}

/// Called with an entry that opens a task of its own.
type Opener = Box<dyn Fn(Entry) + Send + Sync>;

#[derive(Default)]
struct State {
    entries: VecDeque<Entry>,
    /// A task runs.
    busy: bool,
    /// Entries dropped since the reader was last told: the queue was full.
    dropped: usize,
}

/// The queue: its entries in the order they came, whether a task runs, and where an entry that
/// opens a task of its own goes.
pub struct Queue {
    state: Mutex<State>,
    wake: Condvar,
    capacity: usize,
    opener: Option<Opener>,
}

impl Queue {
    /// A queue that keeps at most `capacity` entries.
    pub fn new(capacity: usize) -> Self {
        Self {
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
            capacity: capacity.max(1),
            opener: None,
        }
    }

    /// Where an entry that opens a task goes (the interface, as a task of its own). With none, it
    /// waits for the next task instead.
    #[must_use]
    pub fn on_open(mut self, open: impl Fn(Entry) + Send + Sync + 'static) -> Self {
        self.opener = Some(Box::new(open));
        self
    }

    /// An entry as it comes: kept while a task runs, else opened, kept for the next task or
    /// dropped, as its policy says; a full queue lets its oldest go.
    pub fn post(&self, entry: Entry) {
        let mut state = self.lock();
        if !state.busy {
            match (entry.policy.idle, &self.opener) {
                (Idle::Drops, _) => return,
                (Idle::Opens, Some(open)) => {
                    drop(state);
                    return open(entry);
                }
                (Idle::Opens, None) => {}
            }
        }
        if state.entries.len() >= self.capacity {
            state.entries.pop_front();
            state.dropped += 1;
        }
        state.entries.push_back(entry);
        drop(state);
        self.wake.notify_all();
    }

    /// A task starts: what joins goes to it.
    pub fn begin(&self) {
        self.lock().busy = true;
    }

    /// The task ended: what it did not read opens a task, waits for the next, or goes.
    pub fn end(&self) {
        let mut state = self.lock();
        state.busy = false;
        state
            .entries
            .retain(|entry| entry.policy.idle == Idle::Opens);
        let Some(open) = &self.opener else {
            return;
        };
        let opening = std::mem::take(&mut state.entries);
        drop(state);
        opening.into_iter().for_each(open);
    }

    /// What a running task takes at its step, oldest first, with a line for any dropped.
    pub fn take_joining(&self) -> Vec<Entry> {
        let mut state = self.lock();
        let taken = take(&mut state.entries, |entry| entry.policy.joins);
        let mut out = vec![];
        // The count waits for a take with something to say it with.
        if state.dropped > 0 && !taken.is_empty() {
            let dropped = std::mem::take(&mut state.dropped);
            let note = format!(
                "({dropped} earlier message{} dropped: too many came at once)",
                if dropped == 1 { " was" } else { "s were" }
            );
            out.push(Entry::new(
                Policy {
                    joins: true,
                    idle: Idle::Drops,
                },
                "",
                &note,
            ));
        }
        out.extend(taken);
        out
    }

    /// The entries tagged `tag` of the sources `keys`, oldest first.
    pub fn take_keyed(&self, tag: &str, keys: &[&str]) -> Vec<Entry> {
        take(&mut self.lock().entries, |entry| {
            entry.tag == tag && entry.key.as_deref().is_some_and(|key| keys.contains(&key))
        })
    }

    /// The sources that have entries tagged `tag` waiting.
    pub fn keys(&self, tag: &str) -> Vec<String> {
        let state = self.lock();
        let tagged = state.entries.iter().filter(|entry| entry.tag == tag);
        tagged.filter_map(|entry| entry.key.clone()).collect()
    }

    /// Drop the entries `which` says.
    pub fn forget(&self, which: impl Fn(&Entry) -> bool) {
        self.lock().entries.retain(|entry| !which(entry));
    }

    /// Wait up to `timeout` for an entry to come; not at all when one a running task takes is
    /// there already.
    pub fn wait(&self, timeout: Duration) {
        let state = self.lock();
        if state.entries.iter().any(|entry| entry.policy.joins) {
            return;
        }
        let _ = self.wake.wait_timeout(state, timeout);
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The entries `wanted` picks, taken out of `entries` in their order; the rest stay.
fn take(entries: &mut VecDeque<Entry>, wanted: impl Fn(&Entry) -> bool) -> Vec<Entry> {
    let (taken, kept) = entries.drain(..).partition(wanted);
    *entries = kept;
    Vec::from(taken)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
