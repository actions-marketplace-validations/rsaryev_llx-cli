//! The desk: the lanes of a session at the terminal. Each lane is a whole session of its own
//! (its conversation, agents, jobs and stop) on a thread of its own, over what they all share
//! (the workspace, the session store). The person opens one with `/new` or from the list of saved
//! sessions, and the others go on behind the one shown. The desk ends when the last lane does,
//! and when one panics the others are stopped, so a scope never waits for ever on a lane at its
//! prompt.

mod ending;

use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::Scope;
use std::time::Duration;

use contracts::history::History;
use protocol::{Event, Target};

pub(crate) use self::ending::open_anywhere;
use self::ending::{AllLanes, Lanes, Leaving};
use crate::commands::repl;
use crate::compose::{Rig, Seat, Shared};
use crate::server::Ui;

/// Lanes open at most: opening another closes the oldest idle one.
pub(crate) const MAX_LANES: usize = 8;
/// Saved sessions the list shows at most.
const SAVED: usize = 20;

/// What the interface asks of the desk.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Request {
    /// Open a lane, or show it when it is open already.
    Open(Target),
    /// This directory's saved sessions, for the list.
    List,
    /// "Exit and stop tasks": every lane is stopped, then the process ends.
    Quit,
    /// A client attached from this place: its directory, its environment and its command line.
    Attach(protocol::Place),
    /// "Move to background and exit": the lanes go on when the interface's connection ends.
    Background,
    /// The interface left without keeping the work: it stops, as "Exit and stop tasks" does.
    /// Said where the desk's requests outlive the connection (a daemon keeps them for the next).
    Gone,
}

/// What wakes the desk: a request, the interface's connection gone, a lane's end.
pub(super) enum Wake {
    Asked(Request),
    Gone,
    Left,
}

/// The requests as wakes, on a thread of its own until the interface's connection ends: the
/// desk waits on one channel for them and for its lanes' ends, and never polls.
fn forward(requests: Receiver<Request>, wake: &Sender<Wake>) {
    let forwarder = std::thread::Builder::new().name("llx-desk-requests".into());
    let told = wake.clone();
    let spawned = forwarder.spawn(move || {
        for request in requests {
            if told.send(Wake::Asked(request)).is_err() {
                return;
            }
        }
        let _ = told.send(Wake::Gone);
    });
    // No thread for them: nobody can ask the desk anything, as when the interface is gone.
    if spawned.is_err() {
        let _ = wake.send(Wake::Gone);
    }
}

/// What a lane does once built: its session at the prompt.
pub(crate) type Work<'w> = Box<dyn for<'r, 's> FnOnce(&'r mut Rig<'s>) -> i32 + Send + 'w>;

/// Builds a lane over what is shared, from its seat, and runs its work in it.
pub(crate) type Start<'a> = dyn Fn(&Shared, Seat, Work) -> i32 + Sync + 'a;

/// The session in `ui`: the first lane from `first` (with `task` as its first message), then a
/// lane for each request, until none is left.
pub(crate) fn run(
    shared: &Shared,
    ui: &Ui,
    requests: Receiver<Request>,
    first: Seat,
    task: Option<String>,
    start: &Start<'_>,
    quit: &(dyn Fn() + Sync),
) {
    let lanes = Lanes::default();
    #[expect(
        clippy::disallowed_methods,
        reason = "the desk's wakes: one per request the person makes and per lane's end, each \
                  taken at once"
    )]
    let (wake, woken) = std::sync::mpsc::channel();
    forward(requests, &wake);
    // Moved to the background: the interface's end stops nothing.
    let (mut kept, mut gone) = (false, false);
    std::thread::scope(|scope| {
        let _all = AllLanes { lanes: &lanes, ui };
        let desk = Desk {
            shared,
            lanes: &lanes,
            ui,
            start,
            wake: &wake,
        };
        desk.open(scope, first, task, true);
        while lanes.count() > 0 && !lanes.panicked.load(Ordering::SeqCst) {
            // The desk holds a sender itself: the channel never ends while it waits.
            let Ok(woke) = woken.recv() else { break };
            match woke {
                Wake::Asked(Request::Open(target)) => desk.target(scope, target),
                Wake::Asked(Request::List) => desk.list(),
                Wake::Asked(Request::Quit) => lanes.quit(quit),
                // The first client's place is the desk's, taken before it began: the sessions of
                // a client that attaches later (back to those in the background) stay there.
                Wake::Asked(Request::Attach(_)) => {}
                Wake::Asked(Request::Background) => kept = true,
                // Said where the requests outlive the connection; else the connection's end
                // says it. Unless the person moved the session to the background, its work
                // stops, as "Exit and stop tasks" does.
                Wake::Asked(Request::Gone) | Wake::Gone => {
                    let keep = kept && matches!(woke, Wake::Gone);
                    if !keep && !std::mem::replace(&mut gone, true) {
                        lanes.stop_all();
                    }
                }
                Wake::Left => {}
            }
        }
    });
}

/// What opening a lane needs.
struct Desk<'e> {
    shared: &'e Shared,
    lanes: &'e Lanes,
    ui: &'e Ui,
    start: &'e Start<'e>,
    /// Told when a lane ends.
    wake: &'e Sender<Wake>,
}

impl<'e> Desk<'e> {
    /// Open `target`; a session open already is shown, never opened twice.
    fn target<'s>(&self, scope: &'s Scope<'s, 'e>, target: Target) {
        let (shared, ui) = (self.shared, self.ui);
        if let Target::Saved(id) = &target
            && self.lanes.has(id)
        {
            return ui.send(Event::Show(id.clone()));
        }
        // The interface closed one to make room: it may still be ending.
        if !self
            .lanes
            .wait_until(Duration::from_secs(1), |open| open < MAX_LANES)
        {
            let said = format!("llx: {MAX_LANES} sessions are open: stop or quit one first");
            return ui.send(Event::Error(said));
        }
        match target {
            Target::New => {
                let seat = shared.seat(shared.history.start(), Some(ui.clone()));
                self.open(scope, seat, None, false);
            }
            Target::Saved(id) => match shared.history.load(&id) {
                Ok(session) => {
                    let mut seat = shared.seat(session, Some(ui.clone()));
                    let root = shared.workspace.root();
                    seat.context = crate::context::resumed(&shared.fresh, &seat.session, root);
                    self.open(scope, seat, None, false);
                }
                Err(why) => ui.send(Event::Error(format!("llx: {why}"))),
            },
        }
    }

    /// This directory's saved sessions for the list, newest first, without the agents'.
    fn list(&self) {
        let here = self.shared.workspace.root().display().to_string();
        let saved = crate::commands::sessions::here(self.shared.history.list(), &here);
        let rows = saved.iter().take(SAVED);
        let rows = rows.map(|session| {
            let (title, meta) = crate::commands::sessions::brief(session);
            protocol::Saved {
                id: session.id.clone(),
                title,
                meta,
            }
        });
        self.ui.send(Event::Saved(rows.collect()));
    }

    /// Start a lane on its own thread; one opened after the first is shown at once.
    fn open<'s>(&self, scope: &'s Scope<'s, 'e>, seat: Seat, task: Option<String>, first: bool) {
        let id = seat.session.id.clone();
        if !self.lanes.join(&id) {
            let said =
                format!("llx: session {id} is open at another terminal: quit it there first");
            return self.ui.send(Event::Error(said));
        }
        // A resumed session is known by its title, else by its first task.
        let first_task = seat.session.turns.first().map(|turn| turn.goal.clone());
        let name = Some(seat.session.title.clone())
            .filter(|title| !title.is_empty())
            .or(first_task)
            .unwrap_or_else(|| seat.session.name.clone());
        self.ui.send(Event::LaneStarted {
            id: id.clone(),
            name,
        });
        if !first {
            self.ui.send(Event::Show(id.clone()));
        }
        let (shared, lanes, start) = (self.shared, self.lanes, self.start);
        let wake = self.wake.clone();
        // Outside the thread: when the OS refuses one, the closure drops it, and the lane leaves.
        let leaving = Leaving {
            lanes,
            id: id.clone(),
            wake,
        };
        let thread = std::thread::Builder::new().name("llx-lane".into());
        let started = thread.spawn_scoped(scope, move || {
            let _leaving = leaving;
            let work: Work = Box::new(move |rig| {
                // `--image` goes with the first lane's first task.
                if first {
                    rig.attached = rig.shared.options.images.clone();
                }
                // The desk's interface, tagged with the lane's id by `lane`: every seat here has
                // one, and the lane ends it (what it had waiting goes with it) once its title is
                // in.
                let Some(ui) = rig.ui.clone() else {
                    return crate::exit::OK;
                };
                repl::run(rig, &ui, task.as_deref())
            });
            start(shared, seat, work)
        });
        if let Err(why) = started {
            drop(self.ui.for_lane(&id).ends());
            let said = format!("llx: session {id} could not start (no thread: {why})");
            self.ui.send(Event::Error(said));
        }
    }
}
