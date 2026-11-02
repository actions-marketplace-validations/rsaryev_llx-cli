//! What the interface asks of the server, and what is done. A command names the session it is
//! for (W7): the seam keeps no hidden state.

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// The handshake, first and once (W3).
    Initialize,
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
}

/// What the person sent: the text, and the pictures attached to it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Input {
    pub session: String,
    pub text: String,
    pub images: Vec<PathBuf>,
    /// For after the task running (Tab), not for it.
    pub later: bool,
}

/// The lane [`Command::Open`] opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A new session.
    New,
    /// A session saved before, by its id.
    Saved(String),
}

/// Something running that quitting stops: its kind (`session`, `monitor`, `schedule`) and its
/// name (a session's title, a job's name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    pub kind: String,
    pub name: String,
}

/// What came of a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Done {
    Ok,
    /// The server and what it allows: the commands `/` offers and the modes `/mode` does, each
    /// with what it does.
    Initialized {
        max_lanes: usize,
        commands: Vec<(String, String)>,
        modes: Vec<(String, String)>,
    },
    /// What an interrupt came to: `again` when no task ran or it was stopped already, so the
    /// interface may quit.
    Interrupted {
        again: bool,
    },
    /// What quitting would stop: each thing running, by name.
    Work {
        running: Vec<Running>,
    },
    /// The command could not be done, and why.
    Failed {
        why: String,
    },
}
