//! A session's title, made from its first task while that task runs: the same model at its
//! lowest level, asked once with no tools, its answer cleaned to one short line. The task never
//! waits for it; a failure only leaves the session named by its task.

use contracts::catalog::Selection;
use contracts::chat::{Calling, Chat, Message, Request, TaskId};

use super::Update;
use crate::compose::Rig;
use crate::printable::{one_line, printable};
use protocol::Event;

/// Characters a title takes at most.
const MAX: usize = 36;
/// Bytes of the prompt at most: a long task is cut, on a character.
const PROMPT_BYTES: usize = 960;
/// What the model is told.
const INSTRUCTIONS: &str = "Generate a concise, single-line task title of at most 36 \
characters and under five words where possible. Start with an imperative verb. Capitalize only \
the first word unless the user's language, proper nouns, acronyms, or code terms require \
otherwise. Preserve ticket references exactly. Write in the user's language. Do not use quotes, \
markdown, or trailing punctuation. Do not answer the request. Reply with the title alone.";

/// Name the session of `rig` from `goal` on a thread of its own, in the interface only (one task
/// of `exec` is not listed), when it has no title and none is being asked for. The title goes to
/// the interface and to the saved session as an [`Update`].
pub(crate) fn start(rig: &Rig, goal: &str) {
    let Some(ui) = rig.ui.clone() else { return };
    let Some(naming) = rig.meta.begin(&rig.session) else {
        return;
    };
    let (provider, selection) = (rig.choice.provider.clone(), lowest(&rig.choice.selection));
    let vars = rig.shared.workspace.env().map(<[_]>::to_vec);
    let (pending, id, goal) = (rig.meta.clone(), rig.session.id.clone(), goal.to_string());
    let history = session::Sessions {
        dir: rig.shared.history.dir.clone(),
    };
    let naming_thread = std::thread::Builder::new().name("llx-title".into());
    let started = naming_thread.spawn(move || {
        let _naming = naming;
        let Some(chat) = provider.chat_in(&selection, vars.as_deref()) else {
            return;
        };
        let Some(title) = ask(chat.as_ref(), &goal) else {
            return;
        };
        let update = Update { title: Some(title) };
        let event = Event::Updated(crate::server::wire::update(&update));
        pending.post(&history, &id, update);
        ui.send(event);
    });
    // No thread: the session stays untitled, and the naming ends with the closure, as on a
    // failed request.
    drop(started);
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
        deadline: None,
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
