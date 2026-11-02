//! The session's side of the seam: what it reports, and how it asks and waits. Each report is a
//! frame to the interface, in the session's name; each question goes out with an id and waits
//! for the reply under it (W5); a stopped task withdraws it, and so does the person leaving or
//! the lane closing. Entries come from the server's own queue ([`Inputs`]), never from the
//! interface's state.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, TryRecvError, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use protocol::{Adjustment, Approval, Asked, Event, Group, Reply, ToClient};

use super::hub::{ClientId, Hub};
use super::inputs::{Inputs, Typed};
use super::work::Work;
use crate::picker::{Item, Lines, Picker};
use crate::sync::{Bell, lock};

/// The client [`Peer::new`] attaches.
pub(crate) const FIRST: ClientId = 0;

/// How often a waiting question looks at its stop on its own: a stop by id rings [`ring`], but a
/// stop of the whole process (SIGTERM, the window closed) cannot, from a signal handler.
const STOP_FALLBACK: Duration = Duration::from_secs(1);

/// How often a line asked until something else comes (a sign-in's browser) looks at it: that
/// something rings nothing, and the person is waiting on it.
const DONE_POLL: Duration = Duration::from_millis(100);

/// Rung whenever a waiting question may be over: a reply, a withdrawal, a stop.
static BELL: Bell = Bell::new();

/// Wake every question waiting, to look again at its reply and its stop.
pub(crate) fn ring() {
    BELL.ring();
}

/// A question waiting: the session it is in, and where its reply goes.
type Waiting = (Option<Arc<str>>, SyncSender<Reply>);

/// What the server holds of its interfaces: every client attached and what they were told
/// ([`Hub`]), the questions waiting for a reply, and every lane's entries.
pub(crate) struct Peer {
    pub(crate) hub: Arc<Hub>,
    asked: Mutex<HashMap<u64, Waiting>>,
    next: AtomicU64,
    pub(crate) inputs: Inputs,
    /// What runs in the lanes, for the question asked before quitting.
    pub(crate) work: Work,
}

impl Peer {
    /// A peer with one client attached, `out`, as its first ([`FIRST`]).
    pub(crate) fn new(out: Sender<ToClient>) -> Arc<Self> {
        let hub = Arc::new(Hub::new(super::hub::KEPT, super::hub::KEPT_BYTES));
        let told = Arc::clone(&hub);
        let inputs = Inputs::new(move |lane, event| {
            let session = Some(lane.to_string());
            told.send(ToClient::Event {
                session,
                event,
                seq: None,
            });
        });
        hub.attach(out);
        Arc::new(Self {
            hub,
            asked: Mutex::default(),
            next: AtomicU64::new(1),
            inputs,
            work: Work::default(),
        })
    }

    /// The person's reply to question `id`: the first wins, and every client is told it no
    /// longer waits (W5); a later one is let go.
    pub(crate) fn replied(&self, id: u64, reply: Reply) {
        let waiting = self.waiting().remove(&id);
        if let Some((_, waiting)) = waiting {
            let _ = waiting.send(reply);
            self.send(ToClient::Resolved { id });
            ring();
        }
    }

    /// Nobody will answer what session `lane` (every session, for `None`) waits for: the person
    /// left, or the lane was closed. Its questions end with nothing; each thread that asked one
    /// tells the clients, after its question (R7: told here, it could come first).
    pub(crate) fn abandon(&self, lane: Option<&str>) {
        self.waiting()
            .retain(|_, (session, _)| lane.is_some_and(|lane| session.as_deref() != Some(lane)));
        ring();
    }

    /// `frame` to every client attached.
    pub(crate) fn send(&self, frame: ToClient) {
        self.hub.send(frame);
    }

    fn waiting(&self) -> MutexGuard<'_, HashMap<u64, Waiting>> {
        lock(&self.asked)
    }
}

/// The session's side of the interface: cheap to clone, one per thing that reports or asks.
#[derive(Clone)]
pub(crate) struct Ui {
    peer: Arc<Peer>,
    /// The session whose reports these are; `None` for the server's own.
    lane: Option<Arc<str>>,
}

impl Ui {
    pub(crate) fn new(peer: Arc<Peer>) -> Self {
        Self { peer, lane: None }
    }

    /// The same interface, for what session `id` (a lane or an agent) sends.
    pub(crate) fn for_lane(&self, id: &str) -> Self {
        Self {
            peer: Arc::clone(&self.peer),
            lane: Some(id.into()),
        }
    }

    fn session(&self) -> &str {
        self.lane.as_deref().unwrap_or_default()
    }

    /// Report `event`, in the session's name; what runs in the lanes learns of it.
    pub(crate) fn send(&self, event: Event) {
        self.peer.work.saw(self.lane.as_deref(), &event);
        let session = self.lane.as_deref().map(str::to_string);
        self.peer.send(ToClient::Event {
            session,
            event,
            seq: None,
        });
    }

    /// The person's next entry; `None` when they leave or the lane is closed.
    pub(crate) fn next_entry(&self) -> Option<Typed> {
        self.peer.inputs.next(self.session())
    }

    /// What came back from the background while nothing runs: a task of its own, after what
    /// the person queued.
    pub(crate) fn background(&self, text: &str) {
        self.peer.inputs.background(self.session(), text);
    }

    /// What the person sent to the task running, taken (and shown as theirs).
    pub(crate) fn steered(&self) -> Vec<String> {
        self.peer.inputs.steered(self.session())
    }

    /// Whether the person sent the task running words it has not read.
    pub(crate) fn steering(&self) -> bool {
        self.peer.inputs.steering(self.session())
    }

    /// A guard: when it drops, the lane's entries go with it.
    pub(crate) fn ends(&self) -> Ends {
        Ends(self.clone())
    }

    /// Whether an effect may happen; a stopped task withdraws the question, and it is a no.
    pub(crate) fn approve(
        &self,
        tool: &str,
        summary: &str,
        stopped: &dyn Fn() -> bool,
    ) -> Approval {
        let asked = Asked::Approve {
            tool: tool.into(),
            summary: summary.into(),
        };
        match self.ask(asked, Some((stopped, STOP_FALLBACK))) {
            Some(Reply::Approval(approval)) => approval,
            _ => Approval::No(None),
        }
    }

    /// The `at`th of the model's `questions`; `None` when it was skipped or the task stopped.
    pub(crate) fn question(
        &self,
        questions: &[contracts::person::Question],
        at: usize,
        stopped: &dyn Fn() -> bool,
    ) -> Option<contracts::person::Answer> {
        let question = super::wire::question(questions.get(at)?);
        let headers = match questions.iter().all(|q| q.header.is_empty()) {
            true => vec![],
            false => questions.iter().map(|q| q.header.clone()).collect(),
        };
        let stopped = Some((stopped, STOP_FALLBACK));
        let of = questions.len();
        let asked = Asked::Question {
            question,
            at,
            of,
            headers,
        };
        match self.ask(asked, stopped) {
            Some(Reply::Question(answer)) => answer.map(super::wire::answer),
            _ => None,
        }
    }

    fn typed(&self, prompt: &str, hidden: bool, until: Until<'_>) -> Option<String> {
        let prompt = prompt.into();
        match self.ask(Asked::Line { prompt, hidden }, until) {
            Some(Reply::Line(line)) => line,
            _ => None,
        }
    }

    /// `asked`, and its reply; `None` once it was withdrawn ([`Peer::abandon`]), or once
    /// `until` says so (looked at when rung, and at least as often as it says), which
    /// withdraws it.
    fn ask(&self, asked: Asked, until: Until<'_>) -> Option<Reply> {
        let id = self.peer.next.fetch_add(1, Ordering::Relaxed);
        let (reply, answer) = sync_channel(1);
        let lane = self.lane.clone();
        self.peer.waiting().insert(id, (lane, reply));
        let session = self.session().to_owned();
        self.peer.send(ToClient::Ask { id, session, asked });
        let reply = wait(&answer, until);
        // Withdrawn, by its stop or by `abandon`: the clients are told by the thread that asked,
        // so it always reaches them after the question.
        if reply.is_none() {
            self.peer.waiting().remove(&id);
            self.peer.send(ToClient::Resolved { id });
        }
        reply
    }
}

/// `/login`'s steps (and `/model`'s, `/rewind`'s) in the interface: each list a choice, each line
/// typed in place (a key as dots); `None` when cancelled, or answered with what the list does not
/// offer.
impl Picker for Ui {
    fn pick(&mut self, title: &str, items: &[Item]) -> Option<usize> {
        let (options, marked) = rows(items);
        let title = title.into();
        let asked = Asked::Choose {
            title,
            options,
            marked,
        };
        match self.ask(asked, None) {
            // An index past the list (a client of another build, or one gone wrong) is no
            // choice, as a reply of another kind is none.
            Some(Reply::Choice(picked)) => picked.filter(|&at| at < items.len()),
            _ => None,
        }
    }

    fn pick_adjustable(
        &mut self,
        title: &str,
        items: &[Item],
        adjustments: &[Adjustment],
        groups: &[Group],
    ) -> Option<(usize, usize)> {
        let (options, marked) = rows(items);
        let asked = Asked::ChooseAdjustable {
            title: title.into(),
            options,
            marked,
            adjustments: adjustments.to_vec(),
            groups: groups.to_vec(),
        };
        // A row past the list, or a value past its row's, is no choice, as in `pick`; a row with
        // nothing to adjust keeps its first.
        let offered = |&(at, value): &(usize, usize)| {
            at < items.len()
                && adjustments
                    .get(at)
                    .map_or(value == 0, |adjustment| value < adjustment.values.len())
        };
        match self.ask(asked, None) {
            Some(Reply::Adjusted(picked)) => picked.filter(offered),
            _ => None,
        }
    }
}

impl Lines for Ui {
    fn secret(&mut self, prompt: &str) -> Option<String> {
        self.typed(prompt, true, None)
    }

    fn line_until(&mut self, prompt: &str, done: &dyn Fn() -> bool) -> Option<String> {
        self.typed(prompt, false, Some((done, DONE_POLL)))
    }
}

/// The rows a choice offers (a label and a note each), and the one marked.
fn rows(items: &[Item]) -> (Vec<(String, String)>, Option<usize>) {
    let options = items
        .iter()
        .map(|item| (item.label.clone(), item.detail.clone()))
        .collect();
    (options, items.iter().position(|item| item.marked))
}

/// The lane's end, however it ends: its queue is forgotten.
pub(crate) struct Ends(Ui);

impl Drop for Ends {
    fn drop(&mut self) {
        self.0.peer.inputs.forget(self.0.session());
        self.0.peer.work.ended(self.0.session());
    }
}

/// What ends a question besides its reply, and how often it is looked at unrung.
type Until<'a> = Option<(&'a dyn Fn() -> bool, Duration)>;

/// The reply to the question `answer` waits on: asleep until [`ring`] rings (a reply, a
/// withdrawal, a stop), or `until`'s interval passes.
fn wait(answer: &Receiver<Reply>, until: Until<'_>) -> Option<Reply> {
    loop {
        // Read before looking, so a ring between the look and the wait is not missed.
        let rung = BELL.rung();
        match answer.try_recv() {
            Ok(reply) => return Some(reply),
            Err(TryRecvError::Disconnected) => return None,
            Err(TryRecvError::Empty) => {}
        }
        if until.is_some_and(|(over, _)| over()) {
            return None;
        }
        // A poisoned bell still rings: the next look reads the state again.
        BELL.wait(rung, until.map(|(_, every)| every));
    }
}

#[cfg(test)]
#[path = "ui_tests.rs"]
mod tests;
