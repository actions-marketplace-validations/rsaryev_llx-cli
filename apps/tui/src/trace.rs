//! The `exec` trace on stderr, as a person reads it: what the server tells of a run, the same
//! events the interface receives, drawn with the interface's blocks one after another, with a
//! blank row between two. A pipe or a CI log cannot be redrawn, so there is no live area: a call
//! is drawn once it is done, with the steps that came under it. The binary hands it to the
//! server, which draws nothing itself (K2).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use protocol::Event;
use ratatui::text::{Line, Span};

use super::tool_cell::{self, State};
use super::{cells, style, wrap};

/// The trace's width where stderr is not a terminal.
const WIDTH: usize = 100;

/// The columns of the terminal stderr goes to.
fn stderr_width() -> Option<usize> {
    // SAFETY: a winsize is four integers: all zero is one.
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    // SAFETY: TIOCGWINSZ only fills the struct, which outlives the call.
    let ok = unsafe { libc::ioctl(2, libc::TIOCGWINSZ, &mut size) } == 0;
    (ok && size.ws_col > 0).then_some(usize::from(size.ws_col))
}

/// One run's trace.
pub struct Trace {
    colour: bool,
    width: usize,
    /// How many blocks it drew, the answer on stdout among them: what comes after one is a blank
    /// row from it.
    blocks: Arc<AtomicUsize>,
    /// The steps under the call running (what an agent it started does), drawn with it.
    steps: Vec<String>,
    /// The project: a call is read from it, as the session reads one.
    root: Option<PathBuf>,
}

impl Trace {
    /// For the process's stderr as it is now: its colour and its width.
    pub fn here() -> Self {
        let colour = style::colour_on(&std::io::stderr());
        let trace = Self::new(colour, stderr_width().unwrap_or(WIDTH));
        match std::env::current_dir() {
            Ok(root) => trace.within(&root),
            Err(_) => trace,
        }
    }

    /// The trace of a run in the project `root`.
    #[must_use]
    pub(crate) fn within(mut self, root: &Path) -> Self {
        self.root = Some(root.to_path_buf());
        self
    }

    pub(crate) fn new(colour: bool, width: usize) -> Self {
        Self {
            colour,
            width,
            blocks: Arc::new(AtomicUsize::new(0)),
            steps: vec![],
            root: None,
        }
    }

    /// The rows `event` adds to the trace, each ending in a newline: none for what the trace
    /// does not show.
    pub fn draw(&mut self, event: &Event) -> String {
        let width = self.width;
        match event {
            // Its steps are drawn under it once it is done.
            Event::Call { .. } => {
                self.steps.clear();
                String::new()
            }
            Event::Step(line) => {
                self.steps.push(line.clone());
                String::new()
            }
            Event::Result {
                name,
                label,
                outcome,
                ..
            } => {
                let done = State::Done(outcome);
                let label = super::here::shown(label.clone(), self.root.as_deref());
                let mut text = self.block(&tool_cell::head(name, &label, &done, width));
                for step in std::mem::take(&mut self.steps) {
                    let step = wrap::cut(&format!("  · {step}"), width);
                    text +=
                        &style::rows(&[Line::from(Span::styled(step, style::dim()))], self.colour);
                }
                text + &style::rows(&tool_cell::body(name, outcome, width), self.colour)
            }
            Event::Answer(text) => self.block(&cells::agent(text, width)),
            Event::TaskEnded {
                status,
                millis,
                turns,
                cost,
            } => {
                let end = cells::End {
                    status,
                    millis: *millis,
                    turns: *turns,
                    cost: *cost,
                };
                self.block(&cells::end(&end, width))
            }
            Event::Note(text) => style::rows(&cells::note(text, width), self.colour),
            Event::Error(text) => style::rows(&cells::error(text, width), self.colour),
            Event::Header(_)
            | Event::Mode(_)
            | Event::TaskStarted(_)
            | Event::Earlier(_)
            | Event::Updated(_)
            | Event::LaneStarted { .. }
            | Event::LaneEnded
            | Event::Saved(_)
            | Event::Show(_)
            | Event::Started { .. }
            | Event::Closed
            | Event::Spent(_)
            | Event::Total(_)
            | Event::AgentsCleared
            | Event::Prefill(_)
            | Event::Context(_)
            | Event::Jobs(_)
            | Event::JobOutput { .. }
            | Event::Queue { .. }
            | Event::Steered(_)
            | Event::Copy(_)
            | Event::Quit => String::new(),
        }
    }

    /// How the answer reads on stdout at a terminal: after a blank row, as the model's words are
    /// drawn, at the trace's width and in the colour stdout takes when it is said.
    pub fn answer(&self) -> impl Fn(&str) -> String + Send + Sync + 'static {
        let width = self.width;
        let blocks = Arc::clone(&self.blocks);
        move |text| {
            blocks.fetch_add(1, Ordering::Relaxed);
            let colour = style::colour_on(&std::io::stdout());
            format!("\n{}", style::rows(&cells::agent(text, width), colour))
        }
    }

    /// `lines` after a blank row when blocks came before.
    fn block(&mut self, lines: &[Line]) -> String {
        let before = self.blocks.fetch_add(1, Ordering::Relaxed);
        let gap = if before > 0 { "\n" } else { "" };
        gap.to_string() + &style::rows(lines, self.colour)
    }
}

#[cfg(test)]
#[path = "trace_tests.rs"]
mod tests;
