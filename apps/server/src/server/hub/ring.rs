//! What a hub keeps for a client that attaches later (W6): the last events, numbered, in a ring
//! bounded by count and by bytes; and outside it each open session's whole state (a lane's start,
//! header and mode, an agent's start and close, the latest queue, total, jobs, context and
//! facts), so a client that attaches after the ring let events go still rebuilds every session. A
//! whole state replaces the one before it, in the ring too; a lane that ends takes all of its
//! own, its agents' included, with it.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use protocol::{Event, Label, Outcome, ToClient};

/// A frame as it is kept and posted: shared, so keeping it and replaying it copy nothing.
pub(super) type Frame = Arc<ToClient>;

/// A part of a session's state that an event gives whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Lane,
    Header,
    Mode,
    Agent,
    Closed,
    Queue,
    Total,
    Jobs,
    Context,
    Updated,
}

/// A session's state as its latest events gave it, and the lane that started it, for an agent.
#[derive(Default)]
struct Session {
    parent: Option<String>,
    kept: Vec<(Kind, u64, Frame)>,
}

pub(super) struct Ring {
    /// The numbered events kept, oldest first, each with its weight.
    frames: VecDeque<(u64, Frame, usize)>,
    room: usize,
    bytes: usize,
    held: usize,
    seq: u64,
    /// The newest number the bounds pushed out: a client attaching now misses what it numbers.
    dropped: u64,
    sessions: HashMap<String, Session>,
}

impl Ring {
    pub(super) fn new(room: usize, bytes: usize) -> Self {
        Self {
            frames: VecDeque::new(),
            room,
            bytes,
            held: 0,
            seq: 0,
            dropped: 0,
            sessions: HashMap::new(),
        }
    }

    /// `event` of `session`, numbered and kept: the frame as it is sent.
    pub(super) fn keep(&mut self, session: Option<String>, event: Event) -> Frame {
        self.seq += 1;
        match (&event, &session) {
            (Event::LaneEnded, Some(lane)) => self.end(lane),
            (Event::AgentsCleared, Some(lane)) => self
                .sessions
                .retain(|_, session| session.parent.as_deref() != Some(lane.as_str())),
            _ => {}
        }
        let whole = whole(session.as_deref(), &event);
        let weight = weight(&event);
        let seq = self.seq;
        let frame = Arc::new(ToClient::Event {
            session,
            event,
            seq: Some(seq),
        });
        if let Some((owner, kind, parent)) = whole {
            self.hold(owner, kind, parent, Arc::clone(&frame));
        }
        self.frames.push_back((seq, Arc::clone(&frame), weight));
        self.held += weight;
        while self.frames.len() > self.room || self.held > self.bytes {
            let Some((seq, _, weight)) = self.frames.pop_front() else {
                break;
            };
            self.dropped = seq;
            self.held -= weight;
        }
        frame
    }

    /// What a client gets that attaches: when the ring no longer holds all that happened, a
    /// gap, then every session's state that the ring no longer holds; then the ring.
    pub(super) fn replay(&self) -> Vec<Frame> {
        let mut frames = vec![];
        if 0 < self.dropped {
            frames.push(Arc::new(ToClient::Gap));
            let mut state: Vec<(u64, Frame)> = (self.sessions.values())
                .flat_map(|session| &session.kept)
                .filter(|(_, seq, _)| *seq <= self.dropped)
                .map(|(_, seq, frame)| (*seq, Arc::clone(frame)))
                .collect();
            state.sort_by_key(|(seq, _)| *seq);
            frames.extend(state.into_iter().map(|(_, frame)| frame));
        }
        frames.extend(self.frames.iter().map(|(_, frame, _)| Arc::clone(frame)));
        frames
    }

    /// `frame` as `owner`'s `kind` now; the one before it leaves the ring.
    fn hold(&mut self, owner: String, kind: Kind, parent: Option<String>, frame: Frame) {
        let seq = self.seq;
        let session = self.sessions.entry(owner).or_default();
        if parent.is_some() {
            session.parent = parent;
        }
        let before = session.kept.iter().position(|(of, ..)| *of == kind);
        let before = before.map(|at| session.kept.remove(at).1);
        session.kept.push((kind, seq, frame));
        if let Some(before) = before
            && let Ok(at) = self.frames.binary_search_by_key(&before, |(seq, ..)| *seq)
            && let Some((_, _, weight)) = self.frames.remove(at)
        {
            self.held -= weight;
        }
    }

    /// Lane `lane` ended: nothing of it or of its agents is given again.
    fn end(&mut self, lane: &str) {
        let mut gone: Vec<String> = (self.sessions.iter())
            .filter(|(_, session)| session.parent.as_deref() == Some(lane))
            .map(|(id, _)| id.clone())
            .collect();
        gone.push(lane.to_string());
        self.sessions.retain(|id, _| !gone.contains(id));
        let theirs = |frame: &ToClient| match frame {
            ToClient::Event {
                event: Event::LaneStarted { id, .. } | Event::Show(id),
                ..
            } => id == lane,
            ToClient::Event {
                session: Some(session),
                ..
            } => gone.contains(session),
            ToClient::Event { .. }
            | ToClient::Gap
            | ToClient::Ask { .. }
            | ToClient::Resolved { .. }
            | ToClient::Done { .. } => false,
        };
        self.frames.retain(|(_, frame, weight)| {
            let keep = !theirs(frame);
            if !keep {
                self.held -= weight;
            }
            keep
        });
    }
}

/// Whose state `event` gives whole and which part of it, with the lane that started an agent.
fn whole(session: Option<&str>, event: &Event) -> Option<(String, Kind, Option<String>)> {
    let kind = match event {
        Event::LaneStarted { id, .. } => return Some((id.clone(), Kind::Lane, None)),
        Event::Started { id, .. } => {
            let parent = session.map(str::to_string);
            return Some((id.clone(), Kind::Agent, parent));
        }
        Event::Header(_) => Kind::Header,
        Event::Mode(_) => Kind::Mode,
        Event::Closed => Kind::Closed,
        Event::Queue { .. } => Kind::Queue,
        Event::Total(_) => Kind::Total,
        Event::Jobs(_) => Kind::Jobs,
        Event::Context(_) => Kind::Context,
        Event::Updated(_) => Kind::Updated,
        // Each named, so a new event is placed here, not let through unseen (S10).
        Event::Note(_)
        | Event::Error(_)
        | Event::TaskStarted(_)
        | Event::Call { .. }
        | Event::Result { .. }
        | Event::Step(_)
        | Event::Answer(_)
        | Event::TaskEnded { .. }
        | Event::Earlier(_)
        | Event::LaneEnded
        | Event::Saved(_)
        | Event::Show(_)
        | Event::Spent(_)
        | Event::AgentsCleared
        | Event::Prefill(_)
        | Event::Steered(_)
        | Event::Copy(_)
        | Event::Quit => return None,
    };
    session.map(|session| (session.to_string(), kind, None))
}

/// About how many bytes `event` holds: every text it carries, and a little for the rest. Each
/// event is named, so a new one is weighed for what it carries, never let through light (R3).
fn weight(event: &Event) -> usize {
    let texts = |texts: &[String]| texts.iter().map(String::len).sum::<usize>();
    let label = |label: &Label| label.doing.len() + label.done.len() + label.subject.len();
    let text = match event {
        Event::Note(text)
        | Event::Error(text)
        | Event::Mode(text)
        | Event::TaskStarted(text)
        | Event::Step(text)
        | Event::Answer(text)
        | Event::Show(text)
        | Event::Prefill(text)
        | Event::Copy(text)
        | Event::TaskEnded { status: text, .. } => text.len(),
        Event::Header(rows) | Event::Steered(rows) => texts(rows),
        Event::Call {
            call,
            name,
            label: said,
        } => call.len() + name.len() + label(said),
        Event::Result {
            call,
            name,
            label: said,
            outcome,
        } => {
            let outcome = match outcome {
                Outcome::Done { report, shown } => report.len() + shown.len(),
                Outcome::Failed { why, .. } | Outcome::Fatal { why } => why.len(),
            };
            call.len() + name.len() + label(said) + outcome
        }
        Event::Earlier(turns) => (turns.iter())
            .map(|turn| turn.goal.len() + turn.answer.len() + turn.status.len())
            .sum(),
        Event::Updated(update) => update.title.as_ref().map_or(0, String::len),
        Event::LaneStarted { id, name } | Event::Started { id, name } => id.len() + name.len(),
        Event::Saved(saved) => (saved.iter())
            .map(|saved| saved.id.len() + saved.title.len() + saved.meta.len())
            .sum(),
        Event::Jobs(rows) => (rows.iter())
            .map(|row| {
                let next = row.next.as_ref().map_or(0, String::len);
                row.id.len() + row.name.len() + row.detail.len() + next
            })
            .sum(),
        Event::Queue {
            steered,
            queued,
            background,
            ..
        } => texts(steered) + texts(queued) + texts(background),
        Event::LaneEnded
        | Event::Closed
        | Event::Spent(_)
        | Event::Total(_)
        | Event::AgentsCleared
        | Event::Context(_)
        | Event::Quit => 0,
    };
    text + 64
}
