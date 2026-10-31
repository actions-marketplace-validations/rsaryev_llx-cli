//! What the interface keeps of each session: a lane (a session the person started) or an agent
//! it started, each with its transcript, and which one is shown.

use std::time::Instant;

use ratatui::text::Line;

use super::screen::Block;

/// Every session the interface knows, and which one it shows.
pub(crate) struct Views {
    /// Each lane, followed by the agents it started, in the order they started.
    pub sessions: Vec<SessionView>,
    /// The jobs running in the background, each with the lane whose job it is.
    pub jobs: Vec<(String, protocol::JobRow)>,
    /// The id of the session shown.
    pub shown: String,
    /// The row chosen in the list while it has the keys, in the order the tree draws the
    /// sessions, then the jobs; `None` while the composer has them.
    pub focus: Option<usize>,
    /// The view changed: the screen is cleared before the next frame.
    pub replay: bool,
}

impl Default for Views {
    /// One lane, known by its id once it says it started.
    fn default() -> Self {
        Self {
            sessions: vec![SessionView::new(String::new(), None, String::new())],
            jobs: vec![],
            shown: String::new(),
            focus: None,
            replay: false,
        }
    }
}

/// One session as the interface keeps it: a lane, or an agent with its parent.
#[derive(Default)]
pub(crate) struct SessionView {
    pub id: String,
    /// The lane that started it, for an agent.
    pub parent: Option<String>,
    pub name: String,
    pub blocks: Vec<Block>,
    /// Its calls running, in the order they started.
    pub running: Vec<Running>,
    /// When its task running started.
    pub busy: Option<Instant>,
    /// How long its finished tasks took, in milliseconds.
    pub worked: u64,
    /// The tokens its model turns used.
    pub tokens: u64,
    pub state: AgentState,
    /// A lane's task ended while it was not shown, and it has not been shown since.
    pub unseen: bool,
    /// How many tasks it has finished.
    pub tasks: usize,
    /// What only a lane has: the person's entries and what the task reads; `None` for an agent.
    pub lane: Option<Box<LaneState>>,
}

/// Characters of a first task a lane is named by.
const NAMED: usize = 48;

impl SessionView {
    pub(super) fn new(id: String, parent: Option<String>, name: String) -> Self {
        let lane = parent.is_none().then(|| {
            Box::new(LaneState {
                mode: "default".into(),
                ..LaneState::default()
            })
        });
        Self {
            id,
            parent,
            name,
            lane,
            ..Self::default()
        }
    }

    /// What the person knows it by: its name, else an explicitly new session.
    pub(crate) fn title(&self) -> &str {
        match self.name.is_empty() {
            true => "New session",
            false => &self.name,
        }
    }

    /// Its task began: it works, and the transcript shows what it was asked (not the mode
    /// switch, which is no task of the person's).
    pub(crate) fn started(
        &mut self,
        goal: &str,
        now: Instant,
        width: usize,
    ) -> Option<Vec<Line<'static>>> {
        self.busy = Some(now);
        self.state = AgentState::Working;
        if let Some(lane) = &mut self.lane {
            lane.stopping = false;
        }
        if goal == protocol::SHIFT_TAB {
            return None;
        }
        self.named_by(goal);
        Some(super::cells::user(goal, width))
    }

    /// A lane with no name is named by the first line of its first task.
    pub(crate) fn named_by(&mut self, goal: &str) {
        if self.lane.is_none() || !self.name.is_empty() {
            return;
        }
        let line = goal
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("");
        self.name = super::wrap::cut(line.trim(), NAMED);
    }
}

/// A lane's own: what the person sent it, and what its task reports beside its transcript.
#[derive(Default)]
pub(crate) struct LaneState {
    /// The lane waits for its next entry (the server says so).
    pub ready: bool,
    /// As the server keeps them: the person's words for the task running, their entries for
    /// after it, and what came back from the background, as the transcript will show it.
    pub steered: Vec<String>,
    pub queued: Vec<String>,
    pub background: Vec<String>,
    /// What the session spent in all, as the server last said it.
    pub total: Option<protocol::Total>,
    /// The share of the context window left after the last task, in percent.
    pub context: Option<u8>,
    pub mode: String,
    /// The latest steps under the call running (what an agent it started does).
    pub steps: Vec<String>,
    /// The person stopped the task running.
    pub stopping: bool,
    /// The person said no to a call, and what they said: for the call refused.
    pub declined: Option<String>,
    /// Its own choices, asked while it was not shown: they wait until it is.
    pub held: Vec<(super::select::Select, super::app::Open)>,
}

/// A call running: its id, what it is and how it reads, and when it started.
#[derive(Debug, Clone)]
pub(crate) struct Running {
    pub call: String,
    pub name: String,
    pub label: protocol::Label,
    pub since: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum AgentState {
    /// Started, and given nothing yet.
    #[default]
    Idle,
    Working,
    /// Its last task ended so (`done`, `error`, …).
    Ended(String),
    Closed,
}

impl Views {
    pub(crate) fn find(&self, id: &str) -> Option<usize> {
        self.sessions.iter().position(|session| session.id == id)
    }

    /// A lane began: the interface's first takes its id (it is shown from the start); another
    /// is kept beside the others.
    pub(crate) fn lane_started(&mut self, id: String, name: &str) {
        let first = &mut self.sessions[0];
        if first.id.is_empty() {
            first.id.clone_from(&id);
            first.named_by(name);
            self.shown = id;
        } else if self.find(&id).is_none() {
            let mut lane = SessionView::new(id, None, String::new());
            lane.named_by(name);
            self.sessions.push(lane);
        }
    }

    /// Lane `id` ended: it goes, with its agents and jobs, and the lane to show instead when it
    /// was shown. The last lane stays: the interface ends with it.
    pub(crate) fn lane_ended(&mut self, id: &str) -> Option<String> {
        let at = self
            .find(id)
            .filter(|&at| self.sessions[at].parent.is_none())?;
        let lanes = self.sessions.iter().filter(|view| view.parent.is_none());
        if lanes.count() < 2 {
            return None;
        }
        let shown = self.home() == at;
        let theirs = |view: &SessionView| view.id == id || view.parent.as_deref() == Some(id);
        self.sessions.retain(|view| !theirs(view));
        self.jobs.retain(|(lane, _)| lane != id);
        self.focus = None;
        let next = self.sessions.iter().find(|view| view.parent.is_none());
        next.filter(|_| shown).map(|view| view.id.clone())
    }

    /// The lane of the session at `at`: itself, or the agent's parent.
    pub(crate) fn lane_of(&self, at: usize) -> usize {
        let parent = self.sessions[at].parent.as_deref();
        parent.and_then(|parent| self.find(parent)).unwrap_or(at)
    }

    /// The lane of the session shown.
    pub(crate) fn home(&self) -> usize {
        self.lane_of(self.find(&self.shown).unwrap_or(0))
    }

    /// The sessions the tree draws, in its order: the lane shown and the agents it started, or
    /// none while it has started none. The other lanes are the ← list's.
    pub(crate) fn order(&self) -> Vec<usize> {
        let lane = self.home();
        let id = &self.sessions[lane].id;
        let agents = (0..self.sessions.len())
            .filter(|&at| self.sessions[at].parent.as_ref() == Some(id))
            .collect::<Vec<_>>();
        match agents.is_empty() {
            true => vec![],
            false => std::iter::once(lane).chain(agents).collect(),
        }
    }
}
