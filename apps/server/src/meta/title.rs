//! A session's title, made from its first task while that task runs: asked of what its lane names
//! ([`Namer`]: the same model at its lowest level), once with no tools, its answer cleaned to one
//! short line. The task never waits for it; its thread is the lane's ([`Titling`]), joined when
//! the lane ends. A failure only leaves the session named by its task.

use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use contracts::catalog::Selection;
use contracts::chat::{Calling, Chat, Message, Request, TaskId};

use super::Update;
use crate::choice::Choice;
use crate::compose::Rig;
use crate::printable::{one_line, printable};
use protocol::Event;

/// Characters a title takes at most.
const MAX: usize = 36;
/// Bytes of the prompt at most: a long task is cut, on a character.
const PROMPT_BYTES: usize = 960;
/// How long the title's request may take, its retries included (a host's own timeout is
/// minutes): a title is a few seconds' answer, and the lane's end waits for one still asked for.
const DEADLINE: Duration = Duration::from_secs(15);
/// What the model is told.
const INSTRUCTIONS: &str = "Generate a concise, single-line task title of at most 36 \
characters and under five words where possible. Start with an imperative verb. Capitalize only \
the first word unless the user's language, proper nouns, acronyms, or code terms require \
otherwise. Preserve ticket references exactly. Write in the user's language. Do not use quotes, \
markdown, or trailing punctuation. Do not answer the request. Reply with the title alone.";

/// What a lane asks its titles of: the chat for the choice it thinks with as each title is asked,
/// made on the title's thread. The real lane's is [`namer`]; a test lane's a fake, or none, and
/// then no title is asked.
pub(crate) type Namer = Arc<dyn Fn(&Choice) -> Option<Arc<dyn Chat>> + Send + Sync>;

/// The real lane's [`Namer`]: the choice's own model at its lowest level, from its provider, its
/// keys read in `vars` (the session's environment); made again for each title, from the choice
/// then, so a `/login` switch is followed.
pub(crate) fn namer(vars: crate::providers::Vars) -> Namer {
    let vars = vars.map(<[_]>::to_vec);
    Arc::new(move |choice: &Choice| {
        // A server on the person's machine answers one request at a time, from one cache: a
        // title asked beside the task's first request would wait for it or take its place.
        if choice.provider.on_this_machine(vars.as_deref()) {
            return None;
        }
        let chat = (choice.provider).chat_in(&lowest(&choice.selection), vars.as_deref())?;
        Some(Arc::from(chat))
    })
}

/// Name the session of `rig` from `goal` on a thread of its own, in the interface only (one task
/// of `exec` is not listed), when its lane has a [`Namer`], it has no title and none is being
/// asked for. The title goes to the interface and to the saved session as an [`Update`].
pub(crate) fn start(rig: &mut Rig, goal: &str) {
    let Some(ui) = rig.ui.clone() else { return };
    let Some(namer) = rig.titling.namer.clone() else {
        return;
    };
    let Some(naming) = rig.meta.begin(&rig.session) else {
        return;
    };
    let choice = rig.choice.clone();
    let (kept, goal) = (rig.meta.clone(), goal.to_string());
    let started = rig.titling.start(move || {
        let _naming = naming;
        let Some(chat) = namer(&choice) else {
            return;
        };
        let Some(title) = ask(chat.as_ref(), &goal) else {
            return;
        };
        let update = Update { title: Some(title) };
        let event = Event::Updated(crate::server::wire::update(&update));
        kept.post(&update);
        ui.send(event);
    });
    // No thread: the session stays untitled, and the naming ends with the closure, as on a
    // failed request.
    drop(started);
}

/// What a lane asks its titles of, and the threads that asked, owned by the lane and lent to its
/// [`Rig`] (R2): those that ended are joined as the next starts, and one still asking when the
/// lane ends is waited for then, at most until its request's [`DEADLINE`]. A task never waits for
/// one.
#[derive(Default)]
pub(crate) struct Titling {
    namer: Option<Namer>,
    asking: Vec<JoinHandle<()>>,
}

impl Titling {
    /// A lane's, its titles asked of `namer`; none asked without one.
    pub(crate) fn new(namer: Option<Namer>) -> Self {
        Self {
            namer,
            asking: vec![],
        }
    }

    /// `naming` on a thread of its own, kept with those still asking; those that ended joined.
    fn start(&mut self, naming: impl FnOnce() + Send + 'static) -> std::io::Result<()> {
        let (ended, asking) = self.asking.drain(..).partition(JoinHandle::is_finished);
        self.asking = asking;
        join(ended);
        let thread = std::thread::Builder::new().name("llx-title".into());
        self.asking.push(thread.spawn(naming)?);
        Ok(())
    }
}

impl Drop for Titling {
    fn drop(&mut self) {
        join(std::mem::take(&mut self.asking));
    }
}

/// Wait for each of `threads`; a panic in one was its own, and leaves the session untitled.
fn join(threads: Vec<JoinHandle<()>>) {
    for thread in threads {
        let _ = thread.join();
    }
}

/// The same model at the lowest level it offers: a title needs no thought.
fn lowest(selection: &Selection) -> Selection {
    let offered = selection.model().offered();
    (offered.first())
        .and_then(|&effort| selection.with_effort(effort))
        .unwrap_or_else(|| selection.clone())
}

/// The title `chat` gives the task `goal`; `None` when it fails or says nothing usable.
fn ask(chat: &dyn Chat, goal: &str) -> Option<String> {
    if goal.trim().is_empty() {
        return None;
    }
    let conversation = [
        Message::System(INSTRUCTIONS.into()),
        Message::User(prompt(goal)),
    ];
    let request = Request {
        conversation: &conversation,
        tools: &[],
        calling: Calling::Nothing,
        deadline: Some(Instant::now() + DEADLINE),
        effort: None,
        task: TaskId::NONE,
    };
    clean(chat.complete(request).ok()?.text.as_deref()?)
}

/// The request's text: the task, cut to the budget.
fn prompt(goal: &str) -> String {
    let prompt = format!("Name this task.\n\nUser prompt:\n{}", goal.trim());
    crate::context::cut(&prompt, PROMPT_BYTES).to_string()
}

/// The answer's first line, without a label, quotes, spaces doubled or a closing mark, cut to
/// [`MAX`].
fn clean(said: &str) -> Option<String> {
    let line = said.lines().map(str::trim).find(|line| !line.is_empty())?;
    let line = line.strip_prefix("Title:").unwrap_or(line);
    let quotes: &[char] = &['"', '\'', '`', '“', '”', '‘', '’', '«', '»', '*'];
    let line = line.trim().trim_matches(quotes).trim();
    let line = line
        .trim_end_matches(['.', '?', '!', '。'])
        .trim_matches(quotes);
    let words = one_line(line);
    if words.is_empty() {
        return None;
    }
    let shown = printable(&words);
    Some(match shown.chars().count() > MAX {
        true => {
            shown
                .chars()
                .take(MAX - 1)
                .collect::<String>()
                .trim_end()
                .to_string()
                + "…"
        }
        false => shown,
    })
}

#[cfg(test)]
#[path = "title_tests.rs"]
mod tests;
