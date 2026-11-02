//! What the interface asks of the server, and what is done. A command names the session it is
//! for (W7): the server keeps no state per client but who is attached.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    /// The handshake, first and once (W3), with where the client is: the sessions it opens run
    /// there.
    Initialize {
        version: u32,
        #[serde(default)]
        place: Option<Place>,
        /// The client's build: a daemon of another build refuses it (docs/daemon.md).
        #[serde(default)]
        build: String,
    },
    /// What the person sent a session: its next task, words for the task running, or a command.
    Input(Input),
    /// Ctrl-C: stop the session's task; with none running, or once it was stopped, the person
    /// wants out.
    Interrupt { session: String },
    /// Stop the task of a lane or an agent, and never quit for it.
    Halt { id: String },
    /// Stop a lane's job (a monitor, a schedule).
    StopJob { lane: String, job: String },
    /// Close an idle lane: its prompt is given nothing, and it ends (to make room for another).
    Close { session: String },
    /// Open a lane, or show it when it is open already.
    Open { target: Target },
    /// This directory's saved sessions, for the list.
    List,
    /// "Exit and stop tasks": every lane is stopped, then the work ends.
    QuitAll,
    /// The person left: what waits for them ends with nothing.
    Leave,
    /// What still runs, for the question asked before quitting.
    Work,
    /// "Move to background and exit": the lanes go on with no interface, their prompts and
    /// questions waiting for the next; the connection's end then stops nothing.
    Background,
    /// What the daemon is: its build, its process, who is attached and what is open.
    Status,
    /// The daemon ends: every lane is stopped first.
    Shutdown,
}

/// What the person sent: the text, and the pictures attached to it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    pub session: String,
    pub text: String,
    pub images: Vec<PathBuf>,
    /// For after the task running (Tab), not for it.
    pub later: bool,
}

/// Where a client is: its directory, the environment its commands run with (its `PATH`, an
/// activated virtualenv, its keys).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Place {
    pub cwd: String,
    pub env: Vec<(String, String)>,
    /// The command line it started with, the session's options; its relative paths are of `dir`.
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub dir: String,
}

/// The lane [`Command::Open`] opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum Target {
    /// A new session.
    New,
    /// A session saved before, by its id.
    Saved(String),
}

/// Something running that quitting stops: its kind (`session`, `monitor`, `schedule`) and its
/// name (a session's title, a job's name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Running {
    pub kind: String,
    pub name: String,
}

/// What a daemon says of itself: its build and process, the interfaces attached, the lanes open
/// and working, the desks moved to the background (`kept`), and the monitors and schedules
/// running.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub build: String,
    pub pid: u32,
    pub attached: usize,
    pub lanes: usize,
    pub working: usize,
    /// Absent from an older daemon, as `jobs` is: its lanes open still say whether it is in use.
    #[serde(default)]
    pub kept: usize,
    #[serde(default)]
    pub jobs: usize,
}

impl Status {
    /// Whether nothing needs the daemon: no terminal attached, no session open or moved to the
    /// background, no task working, no monitor or schedule running.
    pub fn idle(&self) -> bool {
        [
            self.attached,
            self.lanes,
            self.working,
            self.kept,
            self.jobs,
        ] == [0; 5]
    }
}

/// What came of a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Done {
    Ok,
    /// The server and what it allows: the commands `/` offers and the modes `/mode` does, each
    /// with what it does.
    Initialized {
        version: u32,
        max_lanes: usize,
        commands: Vec<(String, String)>,
        modes: Vec<(String, String)>,
        /// Whether work may go on once this interface leaves (a daemon's), so quitting may
        /// move it to the background.
        #[serde(default)]
        background: bool,
    },
    /// What an interrupt came to: `again` when no task ran or it was stopped already, so the
    /// interface may quit.
    Interrupted {
        again: bool,
    },
    /// What quitting would stop: each thing running, by name.
    Work {
        #[serde(default)]
        running: Vec<Running>,
    },
    /// What the daemon says of itself.
    Status(Status),
    /// The command could not be done, and why.
    Failed {
        why: String,
    },
}
