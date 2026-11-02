//! The keys the interface takes outside a question: stopping and quitting, sending (now, or for
//! after the task with Tab), the next mode (Shift+Tab) and the reasoning level (Shift+Up/Down),
//! the editor, the clipboard's picture and a suspend, and what `/` and `@` offer.

use std::path::PathBuf;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use protocol::{Command, SHIFT_TAB};

use super::app::{Action, App};
use super::event::Out;
use super::list::stepped;
use super::time::in_quit_window;

impl App {
    pub(super) fn key(&mut self, key: KeyEvent, now: Instant) -> Option<Action> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if !self.questions.is_empty() {
            return self.question_key(key, now);
        }
        if let Some(row) = self.views.focus {
            return self.list_key(key, row, now);
        }
        if self.listing.is_some() {
            return self.sessions_key(key, now);
        }
        if !(ctrl && key.code == KeyCode::Char('c')) {
            self.quitting = None;
        }
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "crossterm's keys, a foreign enum: the others go on to `other_key`"
        )]
        match key.code {
            KeyCode::Char('c') if ctrl => return self.ctrl_c(now),
            // Ctrl-D at an empty prompt quits, asked first while work runs.
            KeyCode::Char('d') if ctrl && self.composer.is_empty() => return self.quit(),
            // Ctrl-Z gives the terminal back and stops llx until the shell's `fg`.
            KeyCode::Char('z') if ctrl => return Some(Action::Suspend),
            // In an agent's view Esc goes back to the main one; Ctrl-C still stops the task.
            KeyCode::Esc if self.shown().parent.is_some() => {
                self.show_lane();
            }
            // Esc twice on an empty prompt with nothing running: rewind.
            KeyCode::Esc if self.busy().is_none() && self.composer.is_empty() => {
                let again = self.escaped.is_some_and(|at| in_quit_window(at, now));
                self.escaped = if again { None } else { Some(now) };
                if again {
                    self.send("/rewind".into(), vec![]);
                }
            }
            // Esc closes the list a `/` opened (not a file list: `@` is part of the words).
            KeyCode::Esc if self.composer.text().starts_with('/') && !self.offers().is_empty() => {
                self.composer.clear();
                self.offered = 0;
            }
            KeyCode::Esc if self.busy().is_some() && !self.lane().stopping => {
                return Some(self.stop());
            }
            KeyCode::Left if self.shown().parent.is_some() && self.opens_sessions(&key) => {
                self.show_lane();
            }
            // ← from an empty prompt lists the sessions in this project.
            KeyCode::Left if self.opens_sessions(&key) => {
                self.open_sessions();
                self.outbox.push(Out::Call(Command::List));
            }
            // ↓ from an empty prompt goes into the list of sessions under it.
            KeyCode::Down if self.enters_list(&key) => self.enter_list(),
            // Tab while a task works keeps the message for after it, instead of sending it now.
            KeyCode::Tab
                if self.busy().is_some()
                    && self.offers().is_empty()
                    && !self.composer.is_empty() =>
            {
                self.queue_for_later();
            }
            KeyCode::BackTab => self.send(SHIFT_TAB.into(), vec![]),
            // The level up or down without a menu; it waits for a task running.
            KeyCode::Up | KeyCode::Down if key.modifiers == KeyModifiers::SHIFT => {
                let step = if key.code == KeyCode::Up {
                    protocol::MORE_REASONING
                } else {
                    protocol::LESS_REASONING
                };
                self.send(step.into(), vec![]);
            }
            KeyCode::Char('v') if ctrl => return Some(Action::Clipboard),
            KeyCode::Char('g') if ctrl => return Some(Action::Editor),
            KeyCode::Tab => {
                let offers = self.offers();
                self.complete(&offers);
            }
            // Among what `/` or `@` offers, once: the files are matched again at every call.
            KeyCode::Up | KeyCode::Down | KeyCode::Enter => {
                let offers = self.offers();
                match offers.len().checked_sub(1) {
                    Some(last) if key.code != KeyCode::Enter => {
                        self.offered = stepped(self.offered, last, key.code);
                    }
                    Some(_) if key.modifiers.is_empty() => return self.enter_offer(key, &offers),
                    Some(_) | None => return self.other_key(key),
                }
            }
            _ => return self.other_key(key),
        }
        None
    }

    /// Ctrl-C, one step back: in an agent's view that agent stops (then the view closes), else
    /// a task running stops, a draft goes, and a second one quits.
    fn ctrl_c(&mut self, now: Instant) -> Option<Action> {
        let shown = self.shown();
        if shown.parent.is_some() {
            if shown.busy.is_some() {
                let id = shown.id.clone();
                self.outbox.push(Out::Call(Command::Halt { id }));
            } else {
                self.show_lane();
            }
            return None;
        }
        // Stopped already, a stop quits: asked first while another session works.
        if self.busy().is_some() && self.lane().stopping && self.others_work() {
            return self.quit();
        }
        if self.busy().is_some() {
            return Some(self.stop());
        }
        // A draft goes, kept for ↑; the next Ctrl-C quits, as at an empty prompt.
        let draft = !self.composer.is_empty();
        if draft {
            self.composer.discard();
        }
        let again = self.quitting.is_some_and(|at| in_quit_window(at, now));
        if again && !draft {
            self.quitting = None;
            return self.quit();
        }
        self.quitting = Some(now);
        self.discarded = draft;
        None
    }

    /// Enter on what `/` or `@` offers.
    fn enter_offer(&mut self, key: KeyEvent, offers: &[Offer]) -> Option<Action> {
        // A command (or a mode) runs as typed when it is a whole one, else the one marked; a
        // file offer inserts its path.
        let typed = self.composer.text();
        let typed = typed.trim();
        let whole = offers.iter().any(|offer| match offer {
            Offer::Command(name, _) => typed == format!("/{name}"),
            Offer::Mode(name, _) => typed.ends_with(&format!(" {name}")),
            Offer::File(_) => false,
        });
        if !whole {
            let entry = match &offers[self.offered.min(offers.len() - 1)] {
                Offer::Command(name, _) => format!("/{name}"),
                Offer::Mode(name, _) => {
                    let command = typed.split_whitespace().next().unwrap_or("/mode");
                    format!("{command} {name}")
                }
                Offer::File(_) => {
                    self.complete(offers);
                    return None;
                }
            };
            self.composer.clear();
            self.offered = 0;
            return self.submit(entry, vec![]);
        }
        self.adopt_paths();
        self.typed(key)
    }

    /// Any other key: the composer's.
    fn other_key(&mut self, key: KeyEvent) -> Option<Action> {
        if key.code == KeyCode::Char('@') {
            self.files.load();
        }
        // A path typed or dropped in without a paste becomes its mark before it is sent.
        if key.code == KeyCode::Enter && key.modifiers.is_empty() {
            self.adopt_paths();
        }
        let action = self.typed(key);
        self.offered = self.offered.min(self.offers().len().saturating_sub(1));
        action
    }

    /// A key for the composer: what it sends goes, with its attachments.
    fn typed(&mut self, key: KeyEvent) -> Option<Action> {
        if let Some(entry) = self.composer.key(key)
            && !entry.trim().is_empty()
        {
            let (text, images) = self.composer.take_attached(&entry);
            return self.submit(text, images);
        }
        None
    }

    /// What the person sent: `/exit` leaves the interface, every lane with it, and `/new` opens
    /// a lane; anything else is the shown lane's.
    fn submit(&mut self, text: String, images: Vec<PathBuf>) -> Option<Action> {
        let command = text.trim().strip_prefix('/').map(str::trim);
        match command {
            Some(name) if protocol::EXIT.contains(&name) => return self.quit(),
            // A new lane opens now: the lane shown goes on in the background.
            Some(name) if protocol::NEW.contains(&name) => {
                self.opening(protocol::Target::New);
            }
            _ => self.send(text, images),
        }
        None
    }

    fn stop(&mut self) -> Action {
        let home = self.views.home();
        if let Some(lane) = self.views.sessions[home].lane.as_deref_mut() {
            lane.stopping = true;
        }
        Action::Stop(self.lane_id().to_string())
    }

    /// What the composer's text offers: the commands its `/` could start, or the files its
    /// `@` word could name.
    pub(crate) fn offers(&self) -> Vec<Offer> {
        if let Some((_, query)) = self.composer.mention() {
            return self
                .files
                .matching(&query)
                .into_iter()
                .map(Offer::File)
                .collect();
        }
        let text = self.composer.text();
        if let Some(typed) = mode_typed(&text) {
            if typed.contains(char::is_whitespace) {
                return vec![];
            }
            return (self.modes.iter())
                .filter(|(name, _)| name.starts_with(typed))
                .map(|(name, what)| Offer::Mode(name.clone(), what.clone()))
                .collect();
        }
        let Some(typed) = text.strip_prefix('/') else {
            return vec![];
        };
        if typed.contains(char::is_whitespace) {
            return vec![];
        }
        self.commands
            .iter()
            .filter(|(name, _)| name.starts_with(typed))
            .map(|(name, what)| Offer::Command(name.clone(), what.clone()))
            .collect()
    }

    /// The marked offer put in: a command replaces the text, a file its `@` word.
    fn complete(&mut self, offers: &[Offer]) {
        let offer = offers.get(self.offered.min(offers.len().saturating_sub(1)));
        match offer {
            Some(Offer::Command(name, _)) => {
                let suffix = if MODE.contains(&name.as_str()) {
                    " "
                } else {
                    ""
                };
                self.composer.set(&format!("/{name}{suffix}"));
            }
            Some(Offer::Mode(name, _)) => {
                let command = self.composer.text();
                let prefix = command.split_whitespace().next().unwrap_or("/mode");
                self.composer.set(&format!("{prefix} {name}"));
            }
            Some(Offer::File(path)) => {
                if let Some((start, _)) = self.composer.mention() {
                    self.composer.replace(start, &format!("{path} "));
                }
            }
            None => {}
        }
        self.offered = 0;
    }
}

/// The commands that take a mode's name after them.
pub(super) const MODE: [&str; 2] = ["mode", "permissions"];

/// What follows a mode command and its space, when `text` starts with one.
pub(super) fn mode_typed(text: &str) -> Option<&str> {
    let typed = text.strip_prefix('/')?;
    MODE.iter()
        .find_map(|name| typed.strip_prefix(name)?.strip_prefix(' '))
}

/// One thing the composer offers to put in.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Offer {
    /// A command's name and what it does.
    Command(String, String),
    /// A mode's name and its effect.
    Mode(String, String),
    /// A workspace file's path.
    File(String),
}
