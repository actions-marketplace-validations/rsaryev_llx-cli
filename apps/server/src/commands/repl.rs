//! A session's prompt, in the interface: `llx` at a terminal, its first message (when one was
//! given) read as any entry is. Each entry is the next task, in the same conversation, workspace
//! and session, or one of the slash commands (`entry.rs`); `/exit` ends the session. The time
//! limit is per task, so waiting for the person to type never times out.

use crate::commands::{login, model};
use crate::compose::Rig;
use crate::exit;
use crate::mode;
use crate::server::Ui;
use protocol::Event;

use super::entry::{Entry, HELP, entry};

/// The session in `ui`, after `first` when it is given.
pub(crate) fn run(rig: &mut Rig, ui: &Ui, first: Option<&str>) -> i32 {
    ui.send(Event::Mode(rig.mode.get().name().to_string()));
    ui.send(header(rig));
    if !rig.session.turns.is_empty() {
        let turns = rig.session.turns.iter().map(crate::server::wire::turn);
        ui.send(Event::Earlier(turns.collect()));
    }
    // The banner's line for a session signed out.
    let provider = &rig.choice.provider;
    if provider.signed_in_by(rig.shared.workspace.env()).is_none() {
        let name = provider.name();
        ui.send(Event::Note(format!(
            "Not signed in to {name}: type /login.\n"
        )));
    }
    if let Some(first) = first
        && !carry_out(rig, ui, first)
    {
        return exit::OK;
    }
    while let Some(typed) = ui.next_entry() {
        if !typed.images.is_empty() {
            rig.attached = typed.images;
        }
        if !carry_out(rig, ui, &typed.text) {
            return exit::OK;
        }
    }
    exit::OK
}

/// Carry out what `text` asks; false once the session is over: `/exit`, or the output's reader
/// gone.
fn carry_out(rig: &mut Rig, ui: &Ui, text: &str) -> bool {
    match entry(text) {
        None => true,
        Some(Entry::Exit) => false,
        // Run the task it asks for, if any; over once the output's reader is gone.
        Some(entry) => command(rig, ui, entry).is_none_or(|task| {
            rig.task(&task);
            !rig.output.closed()
        }),
    }
}

/// The session's header: the version, the model it thinks with, the workspace. Sent again when
/// the model changes, so the line printed last, and the one replayed on attach, names it.
pub(super) fn header(rig: &Rig) -> Event {
    Event::Header(vec![
        env!("CARGO_PKG_VERSION").to_string(),
        rig.choice.label(),
        super::sessions::home_relative(&rig.shared.workspace.root().display().to_string()),
    ])
}

/// Carry out an entry that is not `/exit`; the task it asks to run, if any.
fn command(rig: &mut Rig, ui: &Ui, entry: Entry) -> Option<String> {
    let note = |text: &str| rig.output.note(text);
    match entry {
        Entry::Task(task) => return Some(task),
        Entry::Login => login::run(rig, ui),
        Entry::Model => model::run_model(rig, ui),
        Entry::Rewind => crate::commands::rewind::run(rig, ui),
        Entry::Help => note(HELP),
        Entry::Diff => note(&super::extras::diff(rig.shared.workspace.root())),
        Entry::Copy => {
            if let Some(said) = super::extras::copy(rig) {
                note(said);
            }
        }
        Entry::Usage => note(&super::extras::usage(rig)),
        Entry::Status => note(&status(rig)),
        Entry::Modes => note(&mode::list(&rig.mode.get(), rig.shared.modes.all())),
        Entry::Switch(name) => match rig.shared.modes.named(&name) {
            Some(mode) => {
                note(&mode::switched(&mode));
                rig.switch(mode);
            }
            None => note(&format!(
                "unknown mode {name}: {}\n",
                rig.shared.modes.names()
            )),
        },
        Entry::Reasoning(up) => {
            let said = model::step_reasoning(rig, up);
            note(&said);
        }
        Entry::CycleMode => {
            let mode = rig.shared.modes.after(&rig.mode.get());
            note(&mode::switched(&mode));
            rig.switch(mode);
        }

        Entry::Unknown(command) => {
            note(&format!("unknown command /{command}: /help lists them\n"));
        }
        // The interface opens a lane for `/new` typed at the prompt; one reaches the session
        // only kept for after a task, or as its first message.
        Entry::New => note("Nothing opened: /new opens a session when typed at the prompt.\n"),
        // `carry_out` ends the session first.
        Entry::Exit => {}
    }
    None
}

/// `/status`: what the next task runs with.
pub(crate) fn status(rig: &Rig) -> String {
    format!(
        "model      {} ({})\nworkspace  {}\nmode       {}\nsession    {} ({} tasks)\n",
        rig.choice.label(),
        login::signed(&rig.choice, rig.shared.workspace.env()),
        rig.shared.workspace.root().display(),
        rig.mode.get().name(),
        rig.session.id,
        rig.session.turns.len(),
    )
}

#[cfg(test)]
#[path = "repl_tests.rs"]
mod tests;
