//! Quitting while work runs: Ctrl-C twice, Ctrl-D and `/exit` ask first when something runs (a
//! session's task, a monitor, a schedule), naming each as the server says it. "Exit and
//! stop tasks" stops every session and ends llx; "Move to background and exit", offered only by a
//! server that keeps work once the interface leaves (a daemon's), leaves it running there; "Stay"
//! (or Esc) closes the question.
//!
//! ```text
//! Background work is running
//! The following will stop when you exit:
//!   monitor · cargo build --release
//! ❯ 1. Exit and stop tasks
//!   2. Move to background and exit
//!   3. Stay
//! ```

use std::time::Instant;

use ratatui::text::{Line, Span};

use super::app::{Action, App, Open};
use super::printable::printable;
use super::queue::Asked;
use super::select::{Choice, Picked, Select};
use super::style;

impl App {
    /// The person asks to quit: at once when nothing would be stopped, else asked first.
    pub(super) fn quit(&mut self) -> Option<Action> {
        let running = (self.work)();
        if running.is_empty() {
            return Some(Action::Quit);
        }
        if self.questions.iter().any(|q| matches!(q.open, Open::Quit)) {
            return None;
        }
        let mut title = vec![
            super::questions::heading("Background work is running", ""),
            Line::from(Span::styled(
                "  The following will stop when you exit:",
                style::dim(),
            )),
        ];
        let named = running
            .iter()
            .map(|run| format!("    {} · {}", printable(&run.kind), printable(&run.name)));
        title.extend(named.map(|text| Line::from(Span::styled(text, style::dim()))));
        let mut choices = vec![Choice::new("Exit and stop tasks")];
        if self.background {
            choices.push(Choice::new("Move to background and exit"));
        }
        choices.push(Choice::new("Stay"));
        let select = Select::new(title, choices, "esc stay");
        // The interface's own question, asked by no lane.
        self.questions.push_back(Asked {
            select,
            open: Open::Quit,
            asker: String::new(),
        });
        if self.questions.len() == 1 {
            self.settling = None;
        }
        None
    }

    /// Whether a lane other than the one shown works.
    pub(super) fn others_work(&self) -> bool {
        let home = self.views.home();
        let mut lanes = self.views.sessions.iter().enumerate();
        lanes.any(|(at, view)| at != home && view.lane.is_some() && view.busy.is_some())
    }

    /// The quit question took `picked`: the first exits and stops everything, the second (when
    /// offered) moves the work to the background, anything else stays.
    pub(super) fn quit_answered(&mut self, picked: &Picked, now: Instant) -> Option<Action> {
        self.answer_open(Picked::Cancelled, now);
        match picked {
            Picked::Option(0) => Some(Action::QuitNow),
            Picked::Option(1) if self.background => Some(Action::Background),
            Picked::Option(_)
            | Picked::Adjusted(..)
            | Picked::Many(_)
            | Picked::Typed(..)
            | Picked::Cancelled => None,
        }
    }
}

#[cfg(test)]
#[path = "quitting_tests.rs"]
mod tests;
