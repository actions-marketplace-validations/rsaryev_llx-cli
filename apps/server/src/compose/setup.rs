//! What a session is built from ([`Setup`]), what its lanes share ([`Shared`]), and what one
//! lane is built from beside it ([`Seat`]).

use contracts::history::{History, HistoryError, Record, Session};
use serde_json::Value;
use session::Sessions;
use tools::workspace::Workspace;

use crate::choice::Choice;
use crate::cli::RunOptions;
use crate::mode::Mode;
use crate::modes::Modes;
use crate::output::Drawing;
use crate::server::Ui;

/// Everything a session is built from, decided before any lane starts (so a bad argument
/// still fails fast).
pub(crate) struct Setup {
    pub options: RunOptions,
    /// The provider, model and reasoning level the agent thinks with.
    pub choice: Choice,
    pub workspace: Workspace,
    /// The agent's context block: the environment, the instructions, an earlier session.
    pub context: String,
    /// The same with no earlier session: what `/new` starts over with.
    pub fresh: String,
    pub schema: Option<Value>,
    /// The mode the run starts in, as the context states it.
    pub mode: Mode,
    /// Every mode a session can switch to, the command line's rules joined to each.
    pub modes: Modes,
    /// Whether there is a terminal to ask the person on.
    pub terminal: bool,
    /// Whether `ask` is offered: in a session, not in `exec`, where one task runs unattended.
    pub asks: bool,
    pub history: Sessions,
    /// The session the tasks are recorded in: new, or resumed.
    pub session: Session,
    /// Its writer, held before anything starts (K8).
    pub record: Box<dyn Record>,
    /// How each lane's trace on stderr is drawn: the binary's (none in a test).
    pub drawing: Option<Drawing>,
}

/// What every lane of a session shares: the options, the workspace, the session store, and the
/// fresh context, the same bytes for each lane so the prompt cache is shared too.
pub(crate) struct Shared {
    pub options: RunOptions,
    pub workspace: Workspace,
    pub history: Sessions,
    /// The context with no earlier session: what a new lane starts with.
    pub fresh: String,
    /// The mode a new lane starts in.
    pub started: Mode,
    pub modes: Modes,
    pub choice: Choice,
    pub terminal: bool,
    pub asks: bool,
    /// The person the run has, asked one question at a time (`gate::one_question`, K3), at the
    /// terminal or through the interface.
    pub asking: std::sync::Mutex<()>,
    pub drawing: Option<Drawing>,
}

/// What one lane is built from beside what it shares: it starts in the shared mode and choice.
pub(crate) struct Seat {
    pub context: String,
    pub session: Session,
    /// The session's writer, the lane's while it lasts (K8).
    pub record: Box<dyn Record>,
    /// The interface the lane is shown in: none in `exec`.
    pub ui: Option<Ui>,
    pub schema: Option<Value>,
}

impl Setup {
    /// What the lanes share, and the first lane's seat.
    pub(crate) fn split(self) -> (Shared, Seat) {
        let shared = Shared {
            options: self.options,
            workspace: self.workspace,
            history: self.history,
            fresh: self.fresh,
            started: self.mode,
            modes: self.modes,
            choice: self.choice,
            terminal: self.terminal,
            asks: self.asks,
            asking: Default::default(),
            drawing: self.drawing,
        };
        let seat = Seat {
            context: self.context,
            session: self.session,
            record: self.record,
            ui: None,
            schema: self.schema,
        };
        (shared, seat)
    }
}

impl Shared {
    /// The seat of a new lane, recorded in `session`, which it holds (K8): the fresh context, as
    /// the session began. A session held elsewhere is not opened.
    pub(crate) fn seat(&self, mut session: Session, ui: Option<Ui>) -> Result<Seat, HistoryError> {
        if session.workspace.is_empty() {
            session.workspace = self.workspace.root().display().to_string();
        }
        let record = self.history.hold(&session)?;
        Ok(Seat {
            context: self.fresh.clone(),
            session,
            record,
            ui,
            schema: None,
        })
    }
}
