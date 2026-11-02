//! What is said under the history when the person leaves: usage and how to resume.
//! Only sessions that have a task are worth resuming, so an empty one says nothing.

use ratatui::text::{Line, Span};

use super::app::App;
use super::style;

impl App {
    /// The lines to print once the terminal is given back, the session shown first: the
    /// sessions end with the interface, and each can be resumed from its file.
    pub(crate) fn farewell(&self) -> Vec<Line<'static>> {
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
        sessions
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
            .collect()
    }
}

#[cfg(test)]
#[path = "farewell_tests.rs"]
mod tests;
