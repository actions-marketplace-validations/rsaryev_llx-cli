//! What a run shows as it goes and how it ends. For a person: a trace on stderr (each call once
//! done, an agent's steps under it), which the interface's code draws from the protocol's events
//! ([`Drawn`]), and the answer alone on stdout, so a pipe gets the answer and nothing else. With
//! `--json`: one JSON event per line on stdout ([`json`]). A closed stdout is noticed when
//! something is written to it, never with a panic: with `--json` that is every event, so the run
//! stops at its next turn; otherwise only the answer is written, at the end, and the process then
//! ends quietly.

pub(crate) mod human;
mod json;
pub(crate) mod logged;
mod observer;
mod record;
mod spent;

use std::borrow::Cow;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use contracts::chat::Spend;
use serde_json::Value;

use crate::server::Ui;
use crate::sync::lock;
use protocol::Event;

/// The rows an event adds to the trace on stderr, each ending in a newline.
pub type Draw = Box<dyn FnMut(&Event) -> String + Send>;

/// How the answer reads on stdout at a terminal.
pub type Answer = Box<dyn Fn(&str) -> String + Send + Sync>;

/// The trace as the interface draws it (`tui::Trace`), handed in by the binary: the server tells
/// it what happened as the protocol's events, and draws nothing itself (K2).
pub struct Drawn {
    pub trace: Draw,
    /// `None` where stdout is no terminal: the answer goes as the model wrote it.
    pub answer: Option<Answer>,
}

/// How the binary makes each lane's trace, for the stderr and stdout the process has as the lane
/// opens.
pub type Drawing = fn() -> Drawn;

/// The `--json` event schema: a public contract (see CHANGELOG.md).
pub(crate) const SCHEMA: u32 = 7;

/// What a run starts with, as `run.started` reports it.
pub(crate) struct Start<'a> {
    pub goal: &'a str,
    pub session: &'a str,
    pub model: &'a str,
    pub workspace: &'a str,
}

/// How a run ended, as the closing lines and `run.completed` report it.
pub(crate) struct Summary<'a> {
    /// `done`, `no_data`, `exhausted`, `error`, `interrupted`.
    pub status: &'a str,
    pub code: i32,
    pub answer: &'a str,
    /// The answer was already shown as the model's words (`agent::Run::shown`): the interface is
    /// not sent it again; stdout, `-o` and `--json` have it as always.
    pub shown: bool,
    /// `--schema`: the data, or why none matched.
    pub data: Option<&'a Result<Value, String>>,
    pub error: Option<&'a str>,
    pub agent: &'a Spend,
    pub millis: u64,
    pub session: &'a str,
}

pub(crate) struct Output {
    /// JSON lines on stdout.
    json: bool,
    verbose: bool,
    out: Mutex<Box<dyn Write + Send>>,
    err: Mutex<Box<dyn Write + Send>>,
    started: Mutex<Instant>,
    closed: AtomicBool,
    /// In a session at a terminal: the interface shows the run instead of the trace.
    ui: Option<Ui>,
    /// The trace on stderr: none with `--json`, but with `-v`.
    trace: Option<Mutex<Draw>>,
    /// The answer drawn on stdout, a terminal.
    answer: Option<Answer>,
    /// What the task did so far, each call by its id while it runs: for its session.
    done: Mutex<Vec<(String, contracts::history::Step)>>,
    /// What the session spent in all (`spent.rs`).
    total: Mutex<contracts::history::Spent>,
    /// Each model stream's last request, to tell a prompt cache's miss (`crate::spend`).
    streams: Mutex<crate::spend::Streams>,
    /// `LLX_RECORD`'s directory: each event of the run kept whole (`record.rs`).
    record: Option<std::path::PathBuf>,
    /// Where stderr is read: a CI runner may take a line of it as a command.
    runner: Runner,
    /// The session of the task running, as the log names it: copied out, so no line is written
    /// under the lock (R6).
    session: Mutex<Arc<str>>,
}

/// What reads the trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Runner {
    Plain,
    /// GitHub Actions takes a line that begins `::` or `##[` as a workflow command, on either
    /// stream. Only the trace on stderr is guarded: a file or an answer quoted in it must not be
    /// one, as a runner reads such a line as a command. Stdout is the answer and the `--json`
    /// data, byte-exact for pipes: a caller that lets it reach the runner's log writes it to a
    /// file instead.
    GitHub,
}

impl Runner {
    fn here() -> Self {
        match std::env::var("GITHUB_ACTIONS").as_deref() {
            Ok("true") => Self::GitHub,
            _ => Self::Plain,
        }
    }

    /// `text` as this runner may read it: on GitHub, a zero-width space breaks each command's
    /// opening, which leaves what a person reads the same.
    fn inert(self, text: &str) -> Cow<'_, str> {
        match self {
            Self::Plain => Cow::Borrowed(text),
            Self::GitHub => text
                .split_inclusive('\n')
                .map(|line| {
                    let body = line.trim_start();
                    let (indent, _) = line.split_at(line.len() - body.len());
                    if body.starts_with("::") || body.starts_with("##[") {
                        let (first, rest) = body.split_at(1);
                        format!("{indent}{first}\u{200b}{rest}")
                    } else {
                        line.to_string()
                    }
                })
                .collect(),
        }
    }
}

impl Output {
    /// Output on the process's stdout and stderr, its trace `drawn`.
    pub(crate) fn new(json: bool, verbose: bool, drawn: Option<Drawn>) -> Self {
        let out = Box::new(std::io::stdout());
        let mut output = Self::with(json, verbose, out, Box::new(std::io::stderr())).drawn(drawn);
        output.runner = Runner::here();
        output
    }

    /// Output on given writers: `out` for the answer or the events, `err` for the trace, which
    /// none draws.
    pub(crate) fn with(
        json: bool,
        verbose: bool,
        out: Box<dyn Write + Send>,
        err: Box<dyn Write + Send>,
    ) -> Self {
        Self {
            json,
            verbose,
            out: Mutex::new(out),
            err: Mutex::new(err),
            started: Mutex::new(Instant::now()),
            done: Mutex::default(),
            closed: AtomicBool::new(false),
            ui: None,
            trace: None,
            answer: None,
            total: Mutex::default(),
            streams: Mutex::default(),
            record: record::dir(),
            runner: Runner::Plain,
            session: Mutex::default(),
        }
    }

    /// The run shown in the interface: blocks for the calls and the answer, nothing on stderr.
    pub(crate) fn in_ui(mut self, ui: Option<Ui>) -> Self {
        self.ui = ui;
        self
    }

    /// The trace `drawn`, but with `--json` and no `-v`: the events are then all there is.
    pub(crate) fn drawn(mut self, drawn: Option<Drawn>) -> Self {
        let Some(drawn) = drawn.filter(|_| !self.json || self.verbose) else {
            return self;
        };
        self.trace = Some(Mutex::new(drawn.trace));
        self.answer = drawn.answer;
        self
    }

    /// The session of the task running, as the log names it.
    fn who(&self) -> Arc<str> {
        Arc::clone(&lock(&self.session))
    }

    /// Whether the reader of stdout has gone.
    pub(crate) fn closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// A task starts: the trace's clock restarts.
    pub(crate) fn started(&self, start: &Start) {
        *lock(&self.started) = Instant::now();
        lock(&self.done).clear();
        *lock(&self.session) = Arc::from(start.session);
        self.record(
            "",
            || serde_json::json!({"kind": "task", "goal": start.goal, "model": start.model}),
        );
        logged::started(start);
        if let Some(ui) = &self.ui {
            ui.send(Event::TaskStarted(crate::server::wire::shown(start.goal)));
            return;
        }
        if self.json {
            self.event(&json::started(start));
        }
    }

    /// The interface the run is shown in, at a terminal.
    pub(crate) fn ui(&self) -> Option<&Ui> {
        self.ui.as_ref()
    }

    /// One line under the call running (what an agent it started does): in the interface under
    /// the call, in the trace once the call is done; `--json` has the agent's turns instead.
    pub(crate) fn substep(&self, line: String) {
        let step = Event::Step(line);
        if let Some(ui) = &self.ui {
            return ui.send(step);
        }
        self.draw(&step);
    }

    /// The task ended: the closing lines on stderr and the answer on stdout, or `run.completed`.
    pub(crate) fn completed(&self, summary: &Summary) {
        logged::completed(summary);
        self.record("", || {
            serde_json::json!({"kind": "answer", "status": summary.status, "text": summary.answer})
        });
        let error = summary.error.map(|error| Event::Error(error.to_string()));
        let ended = Event::TaskEnded {
            status: summary.status.to_string(),
            millis: summary.millis,
            turns: summary.agent.calls,
            cost: summary.agent.cost,
        };
        if let Some(ui) = &self.ui {
            if let Some(error) = error {
                ui.send(error);
            }
            if !summary.shown {
                ui.send(Event::Answer(summary.answer.to_string()));
            }
            return ui.send(ended);
        }
        // At a terminal the answer comes first, drawn, then how the run ended; a pipe gets it
        // as the model wrote it.
        let rendered = match (summary.data, &self.answer) {
            (None, Some(answer)) if !self.json => Some(answer),
            _ => None,
        };
        if let Some(answer) = rendered
            && !summary.answer.trim().is_empty()
        {
            self.print(&answer(summary.answer));
        }
        self.draw(&ended);
        self.draw(&Event::Note(human::spend(summary)));
        if let Some(error) = &error {
            self.draw(error);
        }
        if self.json {
            self.event(&json::completed(summary));
            return;
        }
        let (stdout, stderr) = human::answer(summary);
        if !stderr.is_empty() {
            self.note(&stderr);
        }
        if !stdout.is_empty() && rendered.is_none() {
            self.print(&format!("{stdout}\n"));
        }
    }

    /// Lines for the person on stderr, as they are.
    pub(crate) fn note(&self, text: &str) {
        if let Some(ui) = &self.ui {
            return ui.send(Event::Note(text.to_string()));
        }
        let text = self.runner.inert(text);
        let mut err = lock(&self.err);
        let _ = err.write_all(text.as_bytes()).and_then(|()| err.flush());
    }

    /// `event` of `agent`'s (empty: the session's own) in the session's record, when one is kept:
    /// built only then, as it may copy a whole output.
    pub(crate) fn record(&self, agent: &str, event: impl FnOnce() -> Value) {
        if let Some(dir) = &self.record {
            record::write(dir, &self.who(), agent, event());
        }
    }

    /// What the task did since it began, in the order each step began.
    pub(crate) fn take_steps(&self) -> Vec<contracts::history::Step> {
        let mut steps: Vec<_> = lock(&self.done).drain(..).map(|(_, step)| step).collect();
        steps.sort_by_key(|step| step.at_ms);
        steps
    }

    fn millis(&self) -> u64 {
        u64::try_from(lock(&self.started).elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// `event` on the trace, when there is one: as the person's lines (`note`).
    fn draw(&self, event: &Event) {
        let Some(trace) = &self.trace else {
            return;
        };
        // The drawing only computes: its lock is let go before the rows are written.
        let rows = {
            let mut draw = lock(trace);
            (*draw)(event)
        };
        if !rows.is_empty() {
            self.note(&rows);
        }
    }

    fn event(&self, event: &Value) {
        self.print(&format!("{event}\n"));
    }

    /// Write to stdout; once its reader is gone, nothing more is written.
    fn print(&self, text: &str) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let mut out = lock(&self.out);
        if out
            .write_all(text.as_bytes())
            .and_then(|()| out.flush())
            .is_err()
        {
            self.closed.store(true, Ordering::SeqCst);
        }
    }
}

/// `n` of `noun`, as the server says it (the interface counts its own): `1 task`, `3 tasks`.
pub(crate) fn count(n: usize, noun: &str) -> String {
    match n {
        1 => format!("1 {noun}"),
        n => format!("{n} {noun}s"),
    }
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
