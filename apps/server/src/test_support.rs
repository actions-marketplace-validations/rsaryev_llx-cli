//! Test support the binary's tests drive the server with (ARCHITECTURE.md L4, D7): a session at
//! the desk, its lanes thinking with gated models, served in process through the protocol's
//! frames; what an agent's view is sent; a daemon's socket. Built for this crate's tests and, with
//! the `testing` feature, for the binary's, the one crate that knows both the server and the
//! interface; never in the release build. The server's own types stay its own: a test holds the
//! server's side of a desk as a [`Server`].

use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent::observer::Observer;
use clap::Parser;
use contracts::chat::{Chat, ChatError, Completion, Message, Request, ToolCall};
use contracts::history::History;
use protocol::{ToClient, ToServer};
use serde_json::Value;
use tools::workspace::Workspace;

use crate::agents::watch::Watch;
use crate::cli::Cli;
use crate::compose::{Models, Seat, Setup, Shared, lane_with};
use crate::desk::{Work, run};
use crate::mode::Mode;
use crate::output::Output;
use crate::server::host::Host;
use crate::server::serve::{Info, serve};
use crate::server::{Peer, Ui};
use crate::sync::lock;

/// Lanes a desk opens at most, as the interface is told.
pub const MAX_LANES: usize = crate::desk::MAX_LANES;

/// A directory for one test, removed when it ends: tests run in parallel, so each gets its own.
pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new() -> Self {
        Self::under(&std::env::temp_dir())
    }

    #[expect(clippy::unwrap_used, reason = "test support: a failure fails the test")]
    pub(crate) fn under(base: &Path) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let name = format!(
            "llx-cli-{}-{}-{}",
            std::process::id(),
            crate::clock::now(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let dir = base.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        // As the workspace names it (its links resolved: macOS's `/var` is `/private/var`), so
        // paths compare equal with what the app reports.
        let root = Workspace::new(&dir).unwrap();
        Self(root.root().to_path_buf())
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The goals of the tasks session `id` recorded here, in order.
    #[expect(clippy::unwrap_used, reason = "test support: a failure fails the test")]
    pub fn goals(&self, id: &str) -> Vec<String> {
        let history = session::Sessions {
            dir: self.0.join("sessions"),
        };
        let session = history.load(id).unwrap();
        session.turns.into_iter().map(|turn| turn.goal).collect()
    }
}

impl Default for Scratch {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A chat model the test answers one turn at a time (a turn waits until it is sent its reply),
/// and the conversations it was asked to go on from.
pub struct Gated {
    replies: Mutex<Receiver<Completion>>,
    asked: Mutex<Vec<Vec<Message>>>,
}

/// A [`Gated`] model, and where its replies are sent.
pub fn gated() -> (Sender<Completion>, Gated) {
    #[expect(
        clippy::disallowed_methods,
        reason = "a test's script: it sends the turns it means the model to take, then ends"
    )]
    let (replies, received) = channel();
    let chat = Gated {
        replies: Mutex::new(received),
        asked: Mutex::default(),
    };
    (replies, chat)
}

impl Gated {
    /// How many turns it was asked for.
    pub fn turns(&self) -> usize {
        lock(&self.asked).len()
    }

    /// The conversations it was asked to go on from, a turn each.
    pub fn asked(&self) -> Vec<Vec<Message>> {
        lock(&self.asked).clone()
    }
}

impl Chat for Gated {
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        lock(&self.asked).push(request.conversation.to_vec());
        let replies = lock(&self.replies);
        // A test that went wrong ends instead of waiting for ever.
        let waited = replies.recv_timeout(Duration::from_secs(20));
        waited.map_err(|error| ChatError::Unavailable(format!("the test sent no reply: {error}")))
    }
}

/// An answer that says `text` and ends the task: a message with no call.
pub fn says(text: &str) -> Completion {
    Completion {
        text: Some(text.into()),
        ..Completion::default()
    }
}

/// A reply that makes one call of `name` with `arguments`, its id `c-<name>`.
pub fn call(name: &str, arguments: Value) -> Completion {
    Completion {
        calls: vec![ToolCall {
            id: format!("c-{name}"),
            name: name.into(),
            arguments,
        }],
        ..Completion::default()
    }
}

/// A session in `scratch` that starts in `mode`, with no terminal and no interface; its
/// sessions are kept in `scratch` too.
#[expect(clippy::unwrap_used, reason = "test support: a failure fails the test")]
pub(crate) fn setup(scratch: &Scratch, mode: Mode, asks: bool) -> Setup {
    let history = session::Sessions {
        dir: scratch.path().join("sessions"),
    };
    let session = history.start();
    Setup {
        options: Cli::try_parse_from(["llx"]).unwrap().run,
        choice: crate::choice::Choice::new(crate::providers::Provider::openrouter(), "test/model"),
        workspace: Workspace::new(scratch.path()).unwrap(),
        context: String::new(),
        fresh: String::new(),
        schema: None,
        mode,
        modes: crate::modes::Modes::built_in(),
        terminal: false,
        asks,
        history,
        session,
        drawing: None,
    }
}

/// The interface's ends of the in-process transport: what the server sends, and where to send.
pub struct Link {
    pub frames: Receiver<ToClient>,
    pub to_server: Sender<ToServer>,
}

/// The server's side of a desk a test is attached to: what runs in its lanes, and its lanes'
/// inputs, as the next interface reaches them.
pub struct Server(Arc<Peer>);

impl Server {
    /// What runs in the lanes, as the question asked before quitting lists it.
    pub fn running(&self) -> Vec<protocol::Running> {
        self.0.work.running()
    }

    /// An entry for a lane at its prompt, as an interface sends it while no task runs.
    pub fn input(&self, input: protocol::Input) {
        self.0.inputs.input(input, false);
    }

    /// No interface is left: every prompt gets nothing.
    pub fn leave(&self) {
        self.0.inputs.leave();
    }
}

/// A session at the desk, its first task `task`, its lanes thinking with `chats` in the order
/// they open (a lane past them panics as it starts), `quit` for what ends the process when lanes
/// do not stop in time; `person` given the connection to end it as they choose, and the server's
/// side of it; the desk must end within 10 s after (a lane's panic reaches the caller). The
/// session store, for what was recorded.
#[expect(
    clippy::disallowed_methods,
    reason = "the desk's end is looked at again for a bounded time: with the person gone, no \
              frame says it"
)]
#[expect(clippy::unwrap_used, reason = "test support: a failure fails the test")]
pub fn served(
    chats: &[&Gated],
    task: Option<&str>,
    quit: &(dyn Fn() + Sync),
    person: impl FnOnce(Link, &Server),
) -> Scratch {
    let scratch = Scratch::new();
    let bypass = crate::modes::Modes::built_in().named("bypass").unwrap();
    let (shared, seat) = setup(&scratch, bypass, true).split();
    #[expect(
        clippy::disallowed_methods,
        reason = "the in-process transport and the desk's requests, as the binary makes them"
    )]
    let ((to_server, incoming), (out, frames), (to_desk, asked)) =
        (channel(), channel(), channel());
    let peer = Peer::new(out);
    let ui = Ui::new(Arc::clone(&peer));
    let seat = Seat {
        ui: Some(ui.clone()),
        ..seat
    };
    let next = AtomicUsize::new(0);
    let start = |shared: &Shared, seat: Seat, work: Work| {
        let n = next.fetch_add(1, Ordering::SeqCst);
        assert!(n < chats.len(), "a lane the test meant to panic");
        let models = Models {
            chat: chats[n],
            live: None,
            adviser: None,
            searcher: None,
        };
        lane_with(shared, seat, models, work)
    };
    std::thread::scope(|scope| {
        let task = task.map(str::to_string);
        let desk = scope.spawn(|| run(&shared, &ui, asked, seat, task, &start, quit));
        let info = Info {
            max_lanes: MAX_LANES,
            commands: vec![],
            modes: vec![],
            host: None,
        };
        let served = Arc::clone(&peer);
        scope.spawn(move || serve(None, &incoming, &served, &to_desk, &info));
        person(Link { frames, to_server }, &Server(Arc::clone(&peer)));
        let deadline = Instant::now() + Duration::from_secs(10);
        while !desk.is_finished() {
            assert!(
                Instant::now() < deadline,
                "waited too long for the desk to end"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        if let Err(panic) = desk.join() {
            std::panic::resume_unwind(panic);
        }
    });
    scratch
}

/// What an agent's view is sent when agent `id`, started as `name`, says `text` beside its calls.
pub fn said_by_agent(id: &str, name: &str, text: &str) -> Vec<ToClient> {
    #[expect(
        clippy::disallowed_methods,
        reason = "the in-process transport's channel, as the binary makes it"
    )]
    let (out, frames) = channel();
    let ui = Ui::new(Peer::new(out));
    let output = Output::with(
        false,
        false,
        Box::new(std::io::sink()),
        Box::new(std::io::sink()),
    )
    .in_ui(Some(ui));
    let watch = Watch {
        output: &output,
        id,
        name,
    };
    watch.said(text);
    frames.try_iter().collect()
}

/// A listener at `path`, as the daemon binds its socket.
pub fn bound(path: &Path) -> std::io::Result<UnixListener> {
    crate::server::socket::bind(path)
}

/// A daemon of `build` at `path`, serving each connection as `llx serve` does (no lane opens).
pub struct Daemon(Arc<Host>);

impl Daemon {
    pub fn at(path: &Path, build: String) -> std::io::Result<Self> {
        let host = Host::new(build);
        let served = Arc::clone(&host);
        crate::server::socket::accept(bound(path)?, move |connection| {
            #[expect(
                clippy::disallowed_methods,
                reason = "the desk's requests, as the binary makes its channel"
            )]
            let (desk, _requests) = channel();
            let peer = Peer::new(connection.out);
            let info = Info {
                max_lanes: 1,
                commands: vec![],
                modes: vec![],
                host: Some(Arc::clone(&served)),
            };
            serve(None, &connection.incoming, &peer, &desk, &info);
        })?;
        Ok(Self(host))
    }

    /// An interface attached, as the daemon counts them, while what this returns is held.
    pub fn attach(&self) -> impl Sized + use<> {
        self.0.attach()
    }
}
