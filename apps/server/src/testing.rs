//! What the app's unit tests share: a fresh directory per test, a writer whose bytes the test
//! reads back, fakes of the models (no network), a person typing at the terminal, and a whole run
//! composed around them. What the binary's tests share with them is `crate::test_support`'s.

use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use contracts::chat::{Chat, ChatError, Completion, Message, Request, ToolCall};

use crate::compose::{Models, Rig, assemble};
use crate::gate::Prompter;
use crate::interrupt::Interrupt;
use crate::mode::Mode;
use crate::output::Output;
// The binary's tests drive the server with these too.
pub(crate) use crate::test_support::{Scratch, says, setup};

/// What only this crate's tests do with a [`Scratch`].
impl Scratch {
    /// A directory in no git repository, where one can be found: the system's temporary
    /// directory may sit inside one (TMPDIR=$PWD/tmp, a devcontainer), so the usual others are
    /// tried too.
    pub(crate) fn outside_repository() -> Option<Self> {
        [std::env::temp_dir(), "/tmp".into(), "/var/tmp".into()]
            .into_iter()
            .find_map(|base| {
                let scratch = Self::under(&base);
                let in_repository = scratch
                    .path()
                    .ancestors()
                    .any(|dir| dir.join(".git").exists());
                (!in_repository).then_some(scratch)
            })
    }

    /// Write `content` at `name` under the directory, creating its parents.
    pub(crate) fn file(&self, name: &str, content: &str) -> PathBuf {
        let path = self.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }
}

/// A writer that keeps what is written, shared with the test that reads it.
#[derive(Clone, Default)]
pub(crate) struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    pub(crate) fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl Write for Captured {
    #[expect(
        clippy::unwrap_in_result,
        reason = "a test's fake: a poisoned lock fails the test"
    )]
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A writer whose reader has gone: every write is a broken pipe.
pub(crate) struct Closed;

impl Write for Closed {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }
}

/// A chat model that answers from a script, then is unreachable.
pub(crate) struct ScriptedChat(Mutex<Vec<Completion>>);

impl ScriptedChat {
    pub(crate) fn new(replies: Vec<Completion>) -> Self {
        Self(Mutex::new(replies.into_iter().rev().collect()))
    }
}

impl Chat for ScriptedChat {
    #[expect(
        clippy::unwrap_in_result,
        reason = "a test's fake: a poisoned lock fails the test"
    )]
    fn complete(&self, _: Request) -> Result<Completion, ChatError> {
        self.0
            .lock()
            .unwrap()
            .pop()
            .ok_or_else(|| ChatError::Unavailable("the script ran out".into()))
    }
}

/// The terminal, with a person who types lines from a script, then nothing (a closed terminal);
/// it remembers each question shown.
#[derive(Default)]
pub(crate) struct Typed {
    lines: Mutex<VecDeque<&'static str>>,
    pub(crate) asked: Mutex<Vec<String>>,
}

impl Typed {
    pub(crate) fn new(lines: &[&'static str]) -> Self {
        Self {
            lines: Mutex::new(lines.iter().copied().collect()),
            asked: Mutex::default(),
        }
    }
}

impl Prompter for &Typed {
    fn ask(&self, question: &str) -> Option<String> {
        self.asked.lock().unwrap().push(question.to_string());
        self.lines.lock().unwrap().pop_front().map(str::to_string)
    }
}

/// `n` answers that end a task well.
pub(crate) fn answers(n: usize) -> Vec<Completion> {
    (0..n).map(|_| says("Done.")).collect()
}

/// Compose a run that starts in `mode` around a scripted model, in a fresh workspace, and run
/// `work` with it. No terminal: nothing here may wait on a person.
pub(crate) fn composed(
    mode: Mode,
    asks: bool,
    replies: Vec<Completion>,
    work: impl FnOnce(&mut Rig) -> i32,
) {
    composed_with(mode, asks, &ScriptedChat::new(replies), work);
}

/// [`composed`], around `chat`.
pub(crate) fn composed_with(
    mode: Mode,
    asks: bool,
    chat: &dyn Chat,
    work: impl FnOnce(&mut Rig) -> i32,
) {
    let scratch = Scratch::new();
    let (shared, seat) = setup(&scratch, mode, asks).split();
    let models = Models {
        chat,
        live: None,
        adviser: None,
        searcher: None,
    };
    assemble(&shared, seat, models, &quiet(), &Interrupt::new(), work);
}

/// Output nobody reads.
pub(crate) fn quiet() -> Output {
    Output::with(
        false,
        false,
        Box::new(Captured::default()),
        Box::new(Captured::default()),
    )
}

/// What the main conversation's calls came back with, in order: whether each failed, and its
/// report.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
pub(crate) fn results(rig: &Rig) -> Vec<(bool, String)> {
    let conversation = rig.conversation.as_ref().unwrap();
    conversation
        .messages()
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult {
                content, failed, ..
            } => Some((*failed, content.clone())),
            _ => None,
        })
        .collect()
}

/// The user messages of the rig's conversation: the goals and the notes before them.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
pub(crate) fn users(rig: &Rig) -> Vec<String> {
    let conversation = rig.conversation.as_ref().unwrap();
    conversation
        .messages()
        .iter()
        .filter_map(|message| match message {
            Message::User(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// A person who picks from a script, and remembers what each step offered.
#[derive(Default)]
pub(crate) struct Scripted {
    picks: VecDeque<Option<usize>>,
    pub(crate) shown: Vec<(String, Vec<crate::picker::Item>)>,
    /// Each adjustable list's groups: a name and its rows.
    pub(crate) grouped: Vec<Vec<(String, usize)>>,
    /// Each adjustable list's values, row by row.
    pub(crate) adjusted: Vec<Vec<protocol::Adjustment>>,
    /// The level an adjustable pick names, else the row's own.
    pub(crate) level: Option<usize>,
}

impl Scripted {
    pub(crate) fn new(picks: &[Option<usize>]) -> Self {
        Self {
            picks: picks.iter().copied().collect(),
            ..Self::default()
        }
    }

    pub(crate) fn titles(&self) -> Vec<&str> {
        self.shown.iter().map(|(title, _)| title.as_str()).collect()
    }

    pub(crate) fn offered(&self, title: &str) -> Vec<(String, bool)> {
        let (_, items) = self
            .shown
            .iter()
            .find(|(shown, _)| shown.starts_with(title))
            .unwrap();
        items
            .iter()
            .map(|item| (item.label.clone(), item.marked))
            .collect()
    }
}

impl crate::picker::Picker for Scripted {
    fn pick(&mut self, title: &str, items: &[crate::picker::Item]) -> Option<usize> {
        self.shown.push((title.to_string(), items.to_vec()));
        let Some(pick) = self.picks.pop_front() else {
            panic!("no pick was scripted")
        };
        pick
    }

    fn pick_adjustable(
        &mut self,
        title: &str,
        items: &[crate::picker::Item],
        adjustments: &[protocol::Adjustment],
        groups: &[protocol::Group],
    ) -> Option<(usize, usize)> {
        let named = groups.iter().map(|g| (g.name.clone(), g.rows)).collect();
        self.grouped.push(named);
        self.adjusted.push(adjustments.to_vec());
        let at = self.pick(title, items)?;
        let level = self.level.or_else(|| adjustments.get(at).map(|a| a.at));
        Some((at, level.unwrap_or(0)))
    }
}

impl crate::picker::Lines for Scripted {
    fn secret(&mut self, _: &str) -> Option<String> {
        None
    }
}

/// The sessions' side of an interface no one draws: the test reads the frames it is sent.
pub(crate) fn fake_ui() -> (
    crate::server::Ui,
    std::sync::mpsc::Receiver<protocol::ToClient>,
) {
    let (ui, _peer, frames) = fake_peer();
    (ui, frames)
}

/// [`fake_ui`], and the server's side of the interface: replies and entries go through it.
pub(crate) fn fake_peer() -> (
    crate::server::Ui,
    Arc<crate::server::Peer>,
    std::sync::mpsc::Receiver<protocol::ToClient>,
) {
    #[expect(
        clippy::disallowed_methods,
        reason = "the in-process transport's channel, as the binary makes it"
    )]
    let (out, frames) = std::sync::mpsc::channel();
    let peer = crate::server::Peer::new(out);
    (crate::server::Ui::new(Arc::clone(&peer)), peer, frames)
}

/// An interface whose person answers each question with `answer` (nothing for `None`), on a
/// thread of its own until the interface is gone; what was asked, in order, and in which session.
pub(crate) fn answering(
    answer: impl Fn(&protocol::Asked) -> Option<protocol::Reply> + Send + 'static,
) -> (
    crate::server::Ui,
    std::thread::JoinHandle<Vec<(String, protocol::Asked)>>,
) {
    let (ui, peer, frames) = fake_peer();
    // Held weakly: the frames end once the test lets the interface go.
    let peer = Arc::downgrade(&peer);
    let person = std::thread::Builder::new().name("person".into());
    let person = person.spawn(move || {
        let mut asked = vec![];
        for frame in frames {
            if let protocol::ToClient::Ask {
                id,
                session,
                asked: question,
            } = frame
            {
                let reply = answer(&question);
                asked.push((session, question));
                if let (Some(reply), Some(peer)) = (reply, peer.upgrade()) {
                    peer.replied(id, reply);
                }
            }
        }
        asked
    });
    (ui, person.unwrap())
}

/// The events sent so far, each with the session it is in; questions and the rest are left out.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
pub(crate) fn events(
    frames: &std::sync::mpsc::Receiver<protocol::ToClient>,
) -> Vec<(Option<String>, protocol::Event)> {
    let events = frames.try_iter().filter_map(|frame| match frame {
        protocol::ToClient::Event { session, event, .. } => Some((session, event)),
        _ => None,
    });
    events.collect()
}

/// A reply that makes `calls` at once, each its id, its tool's name and its arguments.
pub(crate) fn calls(calls: &[(&str, &str, serde_json::Value)]) -> Completion {
    let calls = calls.iter().map(|(id, name, arguments)| ToolCall {
        id: (*id).into(),
        name: (*name).into(),
        arguments: arguments.clone(),
    });
    Completion {
        calls: calls.collect(),
        ..Completion::default()
    }
}

/// A reply that makes one call, as [`calls`] does.
pub(crate) fn call(id: &str, name: &str, arguments: serde_json::Value) -> Completion {
    calls(&[(id, name, arguments)])
}
