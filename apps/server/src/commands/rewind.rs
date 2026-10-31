//! `/rewind` (also `/undo`, or Esc Esc on an empty prompt): the conversation back to before one
//! of its tasks, the task's words back in the prompt to change. Files stay as they are: the model
//! changes them through the shell, and nothing records a command's files (ARCHITECTURE.md B4).

use crate::compose::Rig;
use crate::picker::{Item, Terminal};
use crate::printable::clipped;
use crate::server::Ui;
use protocol::Event;

/// Characters of a task shown in the list.
const SHOWN: usize = 70;

pub(super) fn run(rig: &mut Rig, ui: &Ui) {
    let said = steps(rig, &mut ui.clone());
    rig.output.note(&said);
}

fn steps(rig: &mut Rig, terminal: &mut dyn Terminal) -> String {
    let tasks = &rig.checkpoints;
    if tasks.is_empty() {
        return "Nothing to rewind: no task yet in this conversation.\n".into();
    }
    // The newest first: the usual rewind is one step back.
    let items: Vec<Item> = tasks
        .iter()
        .rev()
        .map(|task| {
            let shown = crate::server::wire::shown(&task.goal);
            let line = shown.lines().next().unwrap_or("");
            Item::new(clipped(line, SHOWN), "", false)
        })
        .collect();
    let Some(picked) = terminal.pick("Rewind the conversation to before", &items) else {
        return "Nothing changed.\n".into();
    };
    let index = tasks.len() - 1 - picked;
    // The task picked is taken out with those after it: they are forgotten.
    let Some(task) = rig.checkpoints.drain(index..).next() else {
        return "Nothing changed.\n".into();
    };
    match &mut rig.conversation {
        Some(conversation) if task.at > 0 => conversation.truncate(task.at),
        _ => rig.conversation = None,
    }
    rig.agents.forget(rig.output);
    // The person's own words go back in the prompt; what came from the background does not.
    if let Some(ui) = &rig.ui
        && crate::server::wire::shown(&task.goal) == task.goal
    {
        ui.send(Event::Prefill(task.goal));
    }
    "Rewound: the conversation is back to before it; the files stay as they are.\n".into()
}

#[cfg(test)]
#[path = "rewind_tests.rs"]
mod tests;
