//! What is said under the history when the person leaves: usage and how to come back.
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
        let mut sessions: Vec<_> = lanes.filter(worked).collect();
        if let Some(at) = sessions.iter().position(|view| view.id == self.views.shown) {
            sessions[..=at].rotate_right(1);
        }
        let line = |text: String| Line::from(Span::styled(text, style::dim()));
        match leaving {
            Leaving::Quit => sessions
                .into_iter()
                .flat_map(|view| {
                    let usage = view
                        .lane
                        .as_ref()
                        .and_then(|lane| lane.total.as_ref())
                        .map(super::spend::spent)
                        .filter(|usage| !usage.is_empty())
                        .map(|usage| line(format!("Session usage: {usage}")));
                    usage.into_iter().chain(std::iter::once(line(format!(
                        "Resume this session: llx resume {}",
                        view.id
                    ))))
                })
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
