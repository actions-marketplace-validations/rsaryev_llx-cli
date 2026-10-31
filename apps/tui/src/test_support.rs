//! Test-only: an [`App`] driven as the person drives it (events and keys), and read back as the
//! terminal shows it. A frame goes through the real `Screen` into a fixed-size `vt100::Parser`,
//! and the tests assert on the rows a person would read. Spinners and durations are normalized,
//! so a snapshot does not depend on the moment it was drawn.

use std::time::Instant;

use protocol::{Label, Outcome};

use protocol::{Asked, Command, Event, Input as Sent};

use super::app::App;
use super::event::Out;
use super::live;
// What the binary's tests drive the interface with too (`crate::testing`), here for this crate's.
use super::testing::plain;
pub(crate) use super::testing::{Term, ctrl, history, key, press, rows, typed};

/// How a call reads, as the server labels it: the verb while it runs, once done, and its subject.
pub(crate) fn label(doing: &str, done: &str, subject: &str) -> Label {
    Label {
        doing: doing.into(),
        done: done.into(),
        subject: subject.into(),
    }
}

/// How a `bash` call of `command` reads.
pub(crate) fn ran(command: &str) -> Label {
    label("Running", "Ran", command)
}

/// Call `id` of `name` began.
pub(crate) fn call(id: &str, name: &str, label: &Label) -> Event {
    Event::Call {
        call: id.into(),
        name: name.into(),
        label: label.clone(),
    }
}

/// Call `id` of `name` ended with `outcome`.
pub(crate) fn result(id: &str, name: &str, label: &Label, outcome: Outcome) -> Event {
    Event::Result {
        call: id.into(),
        name: name.into(),
        label: label.clone(),
        outcome,
    }
}

/// A call that ended well with `report`.
pub(crate) fn done(report: &str) -> Outcome {
    Outcome::Done {
        report: report.into(),
        shown: String::new(),
    }
}

/// A call that failed, or that the person refused when `declined`.
pub(crate) fn failed(why: &str, declined: bool) -> Outcome {
    Outcome::Failed {
        why: why.into(),
        declined,
    }
}

/// What the person sent the server so far, taken: each entry as `Command::Input` carries it.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
pub(crate) fn sent(app: &mut App) -> Vec<Sent> {
    let mut sent = vec![];
    app.outbox.retain(|out| match out {
        Out::Call(Command::Input(input)) => {
            sent.push(input.clone());
            false
        }
        _ => true,
    });
    sent
}

/// The texts of what the person sent, taken.
pub(crate) fn sent_texts(app: &mut App) -> Vec<String> {
    sent(app).into_iter().map(|input| input.text).collect()
}

/// The replies the person gave so far, taken, each with its question's id.
pub(crate) fn replies(app: &mut App) -> Vec<(u64, protocol::Reply)> {
    let mut replies = vec![];
    app.outbox.retain(|out| match out {
        Out::Reply(id, reply) => {
            replies.push((*id, reply.clone()));
            false
        }
        Out::Call(_) => true,
    });
    replies
}

/// The modes, each with what it does, as the server says them.
pub(crate) fn modes() -> Vec<(String, String)> {
    [
        ("default", "Works in the workspace without asking"),
        ("plan", "Read-only: file changes are refused"),
        ("bypass", "Everything runs without asking (-y)"),
        ("review", "The person's own mode."),
    ]
    .map(|(n, w)| (n.to_string(), w.to_string()))
    .to_vec()
}

/// The commands the interface sent so far other than entries, taken.
pub(crate) fn commands(app: &mut App) -> Vec<Command> {
    let mut commands = vec![];
    app.outbox.retain(|out| match out {
        Out::Call(Command::Input(_)) | Out::Reply(..) => true,
        Out::Call(command) => {
            commands.push(command.clone());
            false
        }
    });
    commands
}

/// Whether an effect of `tool` may happen, as the server asks it.
pub(crate) fn approve(tool: &str, summary: &str) -> Asked {
    Asked::Approve {
        tool: tool.into(),
        summary: summary.into(),
    }
}

/// The lane's queue as the server shows it: whether it waits, the words steered, the entries
/// queued, what came back from the background.
pub(crate) fn queue(ready: bool, steered: &[&str], queued: &[&str], background: &[&str]) -> Event {
    let owned = |texts: &[&str]| texts.iter().map(|t| (*t).to_string()).collect();
    Event::Queue {
        ready,
        steered: owned(steered),
        queued: owned(queued),
        background: owned(background),
    }
}

/// The live area at `now` (what runs, the composer or the question open), as plain text.
pub(crate) fn live_area(app: &App, now: Instant) -> String {
    plain(&live::render(app, now).0)
}

/// `app` drawn once on a fresh terminal: its history not printed yet, then its live area.
pub(crate) fn render(app: &mut App, now: Instant) -> String {
    Term::new(app).draw(app, now)
}

/// `event` as session `id` (a lane or an agent) sends it.
pub(crate) fn in_session(id: &str, event: Event) -> protocol::ToClient {
    protocol::ToClient::Event {
        session: Some(id.into()),
        event,
        seq: None,
    }
}

/// A directory of a test's own, removed when it is dropped.
pub(crate) struct Scratch(std::path::PathBuf);

impl Scratch {
    pub(crate) fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let name = format!("llx-tui-{}-{n}", std::process::id());
        let dir = std::env::temp_dir().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub(crate) fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
