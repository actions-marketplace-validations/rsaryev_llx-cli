//! What is said under the history when the person leaves: how to come back, one dim line each.
//! Only sessions that have a task are worth resuming, so an empty one says nothing.

use ratatui::text::{Line, Span};

use super::app::App;
use super::style;

/// How the interface ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Leaving {
    /// The sessions end with it: each can be resumed from its file.
    Quit,
    /// The server keeps the work: the next `llx` here comes back to it.
    Background,
}

impl App {
    /// The lines to print once the terminal is given back, the session shown first.
    pub(crate) fn farewell(&self, leaving: Leaving) -> Vec<Line<'static>> {
        let lanes = self
            .views
            .sessions
            .iter()
            .filter(|view| view.lane.is_some());
        let worked = |view: &&super::views::SessionView| {
            !view.id.is_empty() && (view.tasks > 0 || view.busy.is_some())
        };
        let mut ids: Vec<&str> = lanes.filter(worked).map(|view| view.id.as_str()).collect();
        if let Some(at) = ids.iter().position(|id| *id == self.views.shown) {
            ids[..=at].rotate_right(1);
        }
        let line = |text: String| Line::from(Span::styled(text, style::dim()));
        match leaving {
            Leaving::Quit => ids
                .into_iter()
                .map(|id| line(format!("Resume this session: llx resume {id}")))
                .collect(),
            // What keeps running may be no task (a shell, a monitor): always said.
            Leaving::Background => vec![line(
                "Still working in the background: run llx in this directory to come back".into(),
            )],
        }
    }
}

#[cfg(test)]
#[path = "farewell_tests.rs"]
mod tests;
