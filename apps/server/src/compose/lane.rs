//! A lane's models, its stop and its output, made before [`assemble`] builds the rest.

use std::sync::Arc;

use contracts::chat::Chat;
use contracts::model::Decider;

use super::{Rig, Seat, Setup, Shared, assemble};
use crate::interrupt::{self, Interrupt};
use crate::live::LiveChat;
use crate::output::Output;

/// The model a session thinks with.
#[derive(Clone, Copy)]
pub(crate) struct Models<'a> {
    pub chat: &'a dyn Chat,
    /// Where `/login` puts the models it chose; `None` when they cannot change.
    pub live: Option<&'a LiveChat>,
    /// The decision model that advises each task's edges (C9); `None`: no advice.
    pub adviser: Option<&'a dyn Decider>,
    /// The decision model `grep` reads the files with (B8); `None`: the files' words alone.
    pub searcher: Option<&'a dyn Decider>,
}

/// Build the agent from `setup` on the chosen provider's chat model, and run `work` with it; its
/// exit code.
pub(crate) fn compose(setup: Setup, work: impl FnOnce(&mut Rig) -> i32) -> i32 {
    let (shared, seat) = setup.split();
    lane(&shared, seat, work)
}

/// One lane over `shared`: its models, its stop (found by its session id), its events tagged
/// with that id (the desk told the interface of it); `work` runs with it.
pub(crate) fn lane(shared: &Shared, seat: Seat, work: impl FnOnce(&mut Rig) -> i32) -> i32 {
    // The session's environment: in the daemon, its client's, whose keys it thinks with.
    let vars = shared.workspace.env();
    let live = LiveChat::of(&shared.choice, vars);
    // Where an OpenRouter key reaches it, unless the person turned it off.
    let adviser = (!shared.options.no_advice)
        .then(|| crate::providers::adviser(vars))
        .flatten();
    // `--no-advice` turns off the advice alone: a search the model asks for is its own.
    let searcher = crate::providers::searcher(vars);
    let models = Models {
        chat: &live,
        live: Some(&live),
        adviser: adviser.as_deref(),
        searcher: searcher.as_deref(),
    };
    lane_with(shared, seat, models, work)
}

/// [`lane`] around `models`.
pub(crate) fn lane_with(
    shared: &Shared,
    mut seat: Seat,
    models: Models,
    work: impl FnOnce(&mut Rig) -> i32,
) -> i32 {
    let interrupt = Arc::new(Interrupt::new());
    let lane = Arc::clone(&interrupt);
    let _known = interrupt::register(&seat.session.id, move || lane.stop());
    seat.ui = seat.ui.map(|ui| ui.for_lane(&seat.session.id));
    let drawn = shared.drawing.map(|drawing| drawing());
    let output = Output::new(shared.options.json, shared.options.verbose, drawn);
    let output = output.in_ui(seat.ui.clone());
    output.spent_before(&seat.session);
    assemble(shared, seat, models, &output, &interrupt, work)
}
