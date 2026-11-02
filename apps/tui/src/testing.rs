//! Test support the binary's tests drive the interface with (ARCHITECTURE.md L4, D7): a person at
//! a real [`App`] fed the server's frames, its commands and replies sent back as the interface's
//! thread sends them, and a window drawn into a `vt100` terminal. Built for this crate's tests
//! and, with the `testing` feature, for the binary's, the one crate that knows both the interface
//! and the server; never in the release build. The [`App`] stays the interface's own: a test reads
//! it through [`Seen`].

use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use protocol::{Command, Event, ToClient, ToServer};

use crate::app::{Action, App};
use crate::event::{Input, Out};
use crate::live;
use crate::screen::{History, Screen};
use crate::style;
use crate::views::AgentState;

/// The person at the interface: a real [`App`] fed what the server sends, its commands and
/// replies sent back, as the interface's thread does.
pub struct Person {
    app: App,
    frames: Receiver<ToClient>,
    to_server: Sender<ToServer>,
    over: bool,
}

/// Where a session stands, as the interface shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Idle,
    Working,
    /// Its task ended: how.
    Ended(String),
    Closed,
}

impl From<&AgentState> for State {
    fn from(state: &AgentState) -> Self {
        match state {
            AgentState::Idle => Self::Idle,
            AgentState::Working => Self::Working,
            AgentState::Ended(how) => Self::Ended(how.clone()),
            AgentState::Closed => Self::Closed,
        }
    }
}

/// What the person sees of the interface at a moment.
pub struct Seen<'a>(&'a App);

impl Seen<'_> {
    /// Whether the lane shown waits for the person's entry.
    pub fn ready(&self) -> bool {
        self.0.lane().ready
    }

    /// Whether the lane shown runs a task.
    pub fn busy(&self) -> bool {
        self.0.busy().is_some()
    }

    /// The id of the session shown.
    pub fn shown(&self) -> &str {
        &self.0.views.shown
    }

    /// The lanes' ids, the first opened first.
    pub fn lanes(&self) -> Vec<String> {
        let sessions = &self.0.views.sessions;
        let lanes = sessions.iter().filter(|view| view.lane.is_some());
        lanes.map(|view| view.id.clone()).collect()
    }

    /// Where session `id` stands, once the interface knows it.
    pub fn state(&self, id: &str) -> Option<State> {
        let at = self.0.views.find(id)?;
        Some(State::from(&self.0.views.sessions[at].state))
    }

    /// Whether session `id`'s task ended.
    pub fn ended(&self, id: &str) -> bool {
        matches!(self.state(id), Some(State::Ended(_)))
    }

    /// Where the session `name` stands (an agent, by the name it was given), once it is known.
    pub fn named(&self, name: &str) -> Option<State> {
        let mut sessions = self.0.views.sessions.iter();
        let view = sessions.find(|view| view.name == name)?;
        Some(State::from(&view.state))
    }
}

impl Person {
    /// A person at an 80 by 24 interface, attached to the server's ends of the in-process
    /// transport, told it may open `max_lanes` lanes.
    pub fn new(frames: Receiver<ToClient>, to_server: Sender<ToServer>, max_lanes: usize) -> Self {
        let mut app = App::new(80, 24, vec![]);
        app.max_lanes = max_lanes;
        Self {
            app,
            frames,
            to_server,
            over: false,
        }
    }

    /// What the person sees now.
    pub fn seen(&self) -> Seen<'_> {
        Seen(&self.app)
    }

    /// Take what the server sends until `done` holds.
    pub fn until(&mut self, what: &str, done: impl Fn(&Seen) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(&self.seen()) {
            let views: Vec<_> = (self.app.views.sessions.iter())
                .map(|v| (v.id.clone(), v.state.clone(), v.busy.is_some()))
                .collect();
            assert!(
                Instant::now() < deadline,
                "waited too long for {what}: {views:?}"
            );
            self.take();
        }
    }

    /// One frame, if one comes soon.
    fn take(&mut self) {
        if let Ok(frame) = self.frames.recv_timeout(Duration::from_millis(20)) {
            let ended = matches!(
                &frame,
                ToClient::Event {
                    event: Event::Quit,
                    ..
                }
            );
            let action = self.app.input(Input::Frame(frame), Instant::now());
            self.act(action, ended);
        }
    }

    /// Take what the server sends for `millis`.
    pub fn wait(&mut self, millis: u64) {
        let until = Instant::now() + Duration::from_millis(millis);
        self.until("time to pass", |_| Instant::now() >= until);
    }

    /// Take what the server sends until the interface is told to end.
    pub fn until_over(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !self.over {
            assert!(Instant::now() < deadline, "waited too long for the end");
            self.take();
        }
    }

    fn act(&mut self, action: Option<Action>, ended: bool) {
        let call = match action {
            Some(Action::Stop(session)) => Some(Command::Interrupt { session }),
            Some(Action::QuitNow) => {
                self.app.leave();
                Some(Command::QuitAll)
            }
            Some(Action::Quit) if ended => {
                self.over = true;
                None
            }
            Some(Action::Quit) => {
                self.app.leave();
                None
            }
            Some(Action::Clipboard | Action::Copy(_) | Action::Editor | Action::Suspend) | None => {
                None
            }
        };
        self.app.outbox.extend(call.map(Out::Call));
        self.flush();
    }

    /// Quit at once, as the person's second Ctrl-C does: "Exit and stop tasks".
    pub fn quit_now(&mut self) {
        self.act(Some(Action::QuitNow), false);
    }

    /// Ctrl-C pressed, and acted on; the session it stops, if it stops one.
    pub fn interrupt(&mut self) -> Option<String> {
        let action = self.app.input(ctrl('c'), Instant::now());
        let stopped = match &action {
            Some(Action::Stop(session)) => Some(session.clone()),
            Some(
                Action::Quit
                | Action::Clipboard
                | Action::Copy(_)
                | Action::Editor
                | Action::Suspend
                | Action::QuitNow,
            )
            | None => None,
        };
        self.act(action, false);
        stopped
    }

    /// The person leaves, as closing the interface does, and says so to the server.
    pub fn leave(&mut self) {
        self.app.leave();
        self.flush();
    }

    /// What the interface has to send, in order.
    fn flush(&mut self) {
        for (id, out) in self.app.outbox.drain(..).enumerate() {
            let frame = match out {
                Out::Call(command) => ToServer::Call {
                    id: id as u64 + 1_000,
                    command,
                },
                Out::Reply(id, reply) => ToServer::Reply { id, reply },
            };
            let _ = self.to_server.send(frame);
        }
    }

    /// Send `command` as the interface would.
    pub fn command(&mut self, command: Command) {
        self.app.outbox.push(Out::Call(command));
        self.flush();
    }

    /// Type `text` and send it.
    pub fn enter(&mut self, text: &str) {
        typed(&mut self.app, text, Instant::now());
        let action = self.app.input(key(KeyCode::Enter), Instant::now());
        self.act(action, false);
    }

    /// The lanes' ids, the first opened first.
    pub fn lanes(&self) -> Vec<String> {
        self.seen().lanes()
    }

    /// Where session `id` stands.
    pub fn state(&self, id: &str) -> Option<State> {
        self.seen().state(id)
    }

    /// The first lane's id, once the interface knows it.
    pub fn first(&mut self) -> String {
        self.until("the first lane", |seen| {
            !seen.0.views.sessions[0].id.is_empty()
        });
        self.app.views.sessions[0].id.clone()
    }

    /// `/new`, and the new lane shown, waiting for its first entry; its id.
    pub fn new_lane(&mut self) -> String {
        let before = self.app.views.shown.clone();
        self.enter("/new");
        self.until("the new lane", |seen| {
            let shown = seen.0.shown();
            let waits = shown.lane.as_ref().is_some_and(|l| l.ready);
            shown.id != before && waits
        });
        self.app.views.shown.clone()
    }

    /// The finished blocks not printed yet, as plain text; taking them prints them.
    pub fn history(&mut self) -> String {
        history(&mut self.app)
    }
}

/// An interface of its own size, fed events and frames and drawn into a `vt100` terminal as the
/// interface's thread draws, at one moment.
pub struct Window {
    app: App,
    term: Term,
    now: Instant,
}

impl Window {
    pub fn new(width: usize, height: usize) -> Self {
        let app = App::new(width, height, vec![]);
        let term = Term::new(&app);
        Self {
            app,
            term,
            now: Instant::now(),
        }
    }

    /// What the server says of itself.
    pub fn event(&mut self, event: Event) {
        self.app.event(event, self.now);
    }

    /// A frame from the server.
    pub fn frame(&mut self, frame: ToClient) {
        self.app.frame(frame, self.now);
    }

    /// `code` pressed.
    pub fn press(&mut self, code: KeyCode) {
        press(&mut self.app, code, self.now);
    }

    /// One frame; the rows the terminal shows then, normalized.
    pub fn draw(&mut self) -> String {
        self.term.draw(&mut self.app, self.now)
    }
}

/// A key pressed with no modifier.
pub(crate) fn key(code: KeyCode) -> Input {
    Input::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

/// Ctrl and a letter.
pub(crate) fn ctrl(c: char) -> Input {
    Input::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
}

/// Press `code` on `app`; what the person asked for, if anything.
pub(crate) fn press(app: &mut App, code: KeyCode, now: Instant) -> Option<Action> {
    app.input(key(code), now)
}

/// Type `text` a character at a time.
pub(crate) fn typed(app: &mut App, text: &str, now: Instant) {
    for c in text.chars() {
        app.input(key(KeyCode::Char(c)), now);
    }
}

/// The finished blocks not printed yet, as plain text; taking them prints them.
pub(crate) fn history(app: &mut App) -> String {
    plain(&rows(app.take_history()))
}

/// The rows of what finished, a blank row after each block, as no chain joins them.
pub(crate) fn rows(history: History) -> Vec<ratatui::text::Line<'static>> {
    let blocks = history.blocks.into_iter();
    blocks
        .flat_map(|block| block.lines.into_iter().chain([Default::default()]))
        .collect()
}

pub(crate) fn plain(lines: &[ratatui::text::Line<'static>]) -> String {
    lines
        .iter()
        .map(style::plain)
        .collect::<Vec<_>>()
        .join("\n")
}

/// A terminal of the app's size, drawn into as the interface's thread draws: what finished goes
/// above once, the live area is redrawn under it, and a changed view clears the screen first.
pub(crate) struct Term {
    screen: Screen<vt100::Parser>,
}

impl Term {
    pub(crate) fn new(app: &App) -> Self {
        let size = |n: usize| u16::try_from(n).unwrap_or(u16::MAX);
        let parser = vt100::Parser::new(size(app.height), size(app.width), 0);
        Self {
            screen: Screen::new(parser, false),
        }
    }

    /// One frame at `now`; the rows the terminal shows then, normalized.
    pub(crate) fn draw(&mut self, app: &mut App, now: Instant) -> String {
        if app.take_replay() {
            let reset = self.screen.reset();
            assert!(reset.is_ok(), "the terminal resets: {reset:?}");
        }
        let history = app.take_history();
        let (live, cursor) = live::render(app, now);
        let drawn = (self.screen).frame(history, &live, cursor, app.height, app.width);
        assert!(drawn.is_ok(), "the frame is drawn: {drawn:?}");
        self.contents()
    }

    /// The rows the terminal shows, without trailing blanks or blank rows, normalized.
    pub(crate) fn contents(&self) -> String {
        let shown = self.screen.output().screen().contents();
        let rows: Vec<&str> = shown.lines().map(str::trim_end).collect();
        normalize(rows.join("\n").trim_end())
    }
}

/// Every spinner frame reads `*`, and every duration (`12s`, `3.0s`, `1m 05s`) reads with `N` in
/// place of its digits.
pub(crate) fn normalize(text: &str) -> String {
    const SPINNER: &str = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if SPINNER.contains(c) {
            out.push('*');
            at += 1;
            continue;
        }
        let starts = c.is_ascii_digit() && (at == 0 || !chars[at - 1].is_alphanumeric());
        if starts {
            let mut end = at;
            while end < chars.len() && (chars[end].is_ascii_digit() || chars[end] == '.') {
                end += 1;
            }
            let unit = chars.get(end).is_some_and(|u| matches!(u, 's' | 'm' | 'h'));
            let alone = !chars.get(end + 1).is_some_and(|n| n.is_alphanumeric());
            if unit && alone && chars[end - 1] != '.' {
                out.push('N');
                at = end;
                continue;
            }
            out.extend(&chars[at..end]);
            at = end;
            continue;
        }
        out.push(c);
        at += 1;
    }
    out
}
