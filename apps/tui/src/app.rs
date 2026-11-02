//! The session's interface as state: what has finished (and is yet to be printed), what runs now,
//! what the person is typing, and the question open. It takes events and keys, says what the
//! person wants done (send an entry, stop the task, quit), and is drawn by [`super::live`].
//! Nothing here touches the terminal, so the tests drive it as the person would.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

use protocol::{Command, Input as Sent, ToClient};

use super::attach;
use super::composer::Composer;
use super::event::{Input, Out};
use super::mention::Files;
use super::questions;
use super::queue::Asked;
use super::screen::{Block, History};
use super::views::{LaneState, Views};
use crate::printable::printable;

/// What the person asked for that the interface cannot do itself.
#[derive(Debug, PartialEq)]
pub(crate) enum Action {
    /// Stop the task of the lane named.
    Stop(String),
    /// Leave the session now.
    Quit,
    /// Attach the clipboard's picture.
    Clipboard,
    /// Put a session's text in the clipboard (`/copy`).
    Copy(String),
    /// Write the composer's text in `$EDITOR`.
    Editor,
    /// Give the terminal back and stop the process (Ctrl+Z), until the shell continues it.
    Suspend,
    /// Quit although work runs: every lane is stopped first ("Exit and stop tasks").
    QuitNow,
}

/// A question open, by the id its reply goes under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Open {
    Approval(u64),
    Question(u64),
    Choice(u64),
    Adjustable(u64),
    Line(u64),
    /// Whether to quit while work runs: the interface's own, nobody waits for it.
    Quit,
}

impl Open {
    /// The id of the server's question; `None` for the interface's own.
    pub(crate) fn id(self) -> Option<u64> {
        match self {
            Self::Approval(id)
            | Self::Question(id)
            | Self::Choice(id)
            | Self::Adjustable(id)
            | Self::Line(id) => Some(id),
            Self::Quit => None,
        }
    }
}

pub(crate) struct App {
    pub(super) width: usize,
    pub(super) height: usize,
    /// Finished blocks not printed yet.
    pub(super) pending: Vec<Block>,
    /// The main session and its agents, each a transcript, and the one shown.
    pub(crate) views: Views,
    pub(super) composer: Composer,
    /// The commands `/` offers: each name and what it does.
    pub(super) commands: Vec<(String, String)>,
    /// The command marked among those offered.
    pub(super) offered: usize,
    /// The workspace's files, for `@`.
    pub(super) files: Files,
    /// Where pictures dropped in are kept.
    pub(super) pasted: PathBuf,
    /// The question open first, then those waiting their turn.
    pub(super) questions: VecDeque<Asked>,
    /// Until when the question open ignores keys: what was typed ahead for the composer as it
    /// opened must not answer it.
    pub(super) settling: Option<Instant>,
    /// Ctrl-C at an empty prompt: a second one quits, until this moment.
    pub(super) quitting: Option<Instant>,
    /// That Ctrl-C cleared a draft, which ↑ brings back.
    pub(super) discarded: bool,
    /// Esc at an empty prompt: a second one rewinds, until this moment.
    pub(super) escaped: Option<Instant>,
    /// The person left: the sessions' next waits end with nothing.
    pub(super) left: bool,
    /// The list of sessions, while it is open.
    pub(super) listing: Option<super::sessions::SessionList>,
    /// This directory's saved sessions, as the desk last listed them.
    pub(super) saved: Vec<protocol::Saved>,
    /// A lane closed to make room, said in the next lane that opens.
    pub(super) closed: Option<String>,
    /// What runs, in every session (asked of the server): quitting stops it.
    pub(super) work: Box<dyn Fn() -> Vec<protocol::Running> + Send>,
    /// The modes `/mode` offers, each with what it does, as the server says them.
    pub(super) modes: Vec<(String, String)>,
    /// Lanes the server keeps open at most.
    pub(crate) max_lanes: usize,
    /// What the interface sends next: commands and replies, in order.
    pub(crate) outbox: Vec<Out>,
}

impl App {
    pub(crate) fn new(width: usize, height: usize, commands: Vec<(String, String)>) -> Self {
        Self {
            width,
            height,
            pending: vec![],
            views: Views::default(),
            composer: Composer::default(),
            commands,
            offered: 0,
            files: Files::default(),
            pasted: attach::pasted_dir(),
            questions: VecDeque::new(),
            settling: None,
            quitting: None,
            discarded: false,
            escaped: None,
            left: false,
            listing: None,
            saved: vec![],
            closed: None,
            work: Box::new(Vec::new),
            modes: vec![],
            max_lanes: 8,
            outbox: vec![],
        }
    }

    /// The finished blocks to print, and whether a question is open; they are printed once.
    pub(crate) fn take_history(&mut self) -> History {
        History {
            blocks: std::mem::take(&mut self.pending),
            asking: !self.questions.is_empty(),
        }
    }

    /// The lane of the session shown: what the person types goes there.
    #[expect(
        clippy::expect_used,
        reason = "`home` is a lane: an agent's view is its parent's, and the first view is a lane"
    )]
    pub(crate) fn lane(&self) -> &LaneState {
        let lane = self.views.sessions[self.views.home()].lane.as_deref();
        lane.expect("a lane")
    }

    /// When the task of the lane shown started, while it runs.
    pub(crate) fn busy(&self) -> Option<Instant> {
        self.views.sessions[self.views.home()].busy
    }

    /// What the lane shown is known by: an agent's view names it.
    pub(crate) fn home_title(&self) -> String {
        printable(self.views.sessions[self.views.home()].title())
    }

    /// The id of the lane shown: its stop is known by it.
    pub(crate) fn lane_id(&self) -> &str {
        &self.views.sessions[self.views.home()].id
    }

    /// Show the lane itself, out of one of its agents' views.
    pub(super) fn show_lane(&mut self) {
        let lane = self.lane_id().to_string();
        self.show(&lane);
    }

    /// What the interface's thread took: a frame from the server, or the terminal's.
    pub(crate) fn input(&mut self, input: Input, now: Instant) -> Option<Action> {
        match input {
            Input::Frame(frame) => self.frame(frame, now),
            Input::Key(key) => self.key(key, now),
            Input::Paste(text) => self.paste(&text),
            Input::Resize(width, height) => {
                let was = self.width;
                (self.width, self.height) = (usize::from(width), usize::from(height));
                // The terminal reflows what it shows at a new width, so the rows above the
                // cursor are no longer the rows drawn: the transcript is drawn again at it.
                if self.width != was {
                    self.redraw();
                }
                None
            }
            Input::Files(list) => {
                self.files.loaded(list);
                None
            }
        }
    }

    /// A frame from the server: an event of a session or of the server, a question, a
    /// question withdrawn.
    pub(crate) fn frame(&mut self, frame: ToClient, now: Instant) -> Option<Action> {
        match frame {
            ToClient::Event {
                session: Some(id),
                event,
                ..
            } => self.event_in(&id, event, now),
            ToClient::Event {
                session: None,
                event,
                ..
            } => self.event(event, now),
            ToClient::Ask { id, session, asked } => {
                self.asked(id, &session, asked, now);
                None
            }
            ToClient::Resolved { id } => {
                self.resolved(id, now);
                None
            }
            // What a command came to is the caller's (`Client::call`).
            ToClient::Done { .. } => None,
        }
    }

    /// The server asks the person, for `session`'s task: an approval and the model's questions
    /// wait their turn; a choice of the lane's own waits until it is shown.
    pub(crate) fn asked(&mut self, id: u64, session: &str, asked: protocol::Asked, now: Instant) {
        let at = (self.views.find(session)).unwrap_or_else(|| self.views.home());
        let lane = self.views.lane_of(at);
        let label = self.label(lane);
        match asked {
            protocol::Asked::Approve { tool, summary } => {
                let select = questions::approval(&tool, &summary, label.as_deref());
                self.ask(lane, select, Open::Approval(id), now);
            }
            protocol::Asked::Question {
                question,
                at,
                of,
                headers,
            } => {
                let select = questions::question(&question, (at, of), &headers, label.as_deref());
                self.ask(lane, select, Open::Question(id), now);
            }
            protocol::Asked::Choose {
                title,
                options,
                marked,
            } => {
                let select = questions::choice(&title, &options, marked);
                self.ask_own(lane, select, Open::Choice(id), now);
            }
            protocol::Asked::ChooseAdjustable {
                title,
                options,
                marked,
                adjustments,
                groups,
            } => {
                let select = questions::choice(&title, &options, marked)
                    .adjustable(adjustments)
                    .grouped(&groups);
                self.ask_own(lane, select, Open::Adjustable(id), now);
            }
            protocol::Asked::Line { prompt, hidden } => {
                let select = questions::line(&prompt, hidden);
                self.ask_own(lane, select, Open::Line(id), now);
            }
        }
    }

    /// What the terminal pasted: into the question open, else the composer. A dropped file
    /// arrives as a path; other clipboard text is shown as [Paste].
    fn paste(&mut self, text: &str) -> Option<Action> {
        if let Some(asked) = self.questions.front_mut() {
            asked.select.paste(text);
        } else if Composer::is_path_list(text, &attach::resolve) {
            self.composer.paste(text);
            self.adopt_paths();
        } else {
            self.composer.paste_marked(text);
        }
        None
    }

    /// An entry for the lane shown: the server gives it to the lane at its prompt, to the task
    /// running (words), or keeps it for after.
    pub(super) fn send(&mut self, text: String, images: Vec<PathBuf>) {
        // What the person types is for the lane: it is shown again to see it go there.
        self.show_lane();
        self.tell(text, images, false);
    }

    /// An entry to the lane shown, `later` when it is for after the task running.
    pub(super) fn tell(&mut self, text: String, images: Vec<PathBuf>, later: bool) {
        let session = self.lane_id().to_string();
        let input = Sent {
            session,
            text,
            images,
            later,
        };
        self.outbox.push(Out::Call(Command::Input(input)));
    }

    /// Each path in the composer that names a file becomes its mark; a picture is kept a copy of.
    pub(crate) fn adopt_paths(&mut self) {
        let keep = |path: &std::path::Path| attach::keep(path, &self.pasted);
        self.composer.adopt(&attach::resolve, &keep);
    }

    /// The person is leaving: what waits for them ends with nothing.
    pub(crate) fn leave(&mut self) {
        self.left = true;
        self.outbox.push(Out::Call(Command::Leave));
        self.close_questions();
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
