//! A job that ended, a command in the background or a monitor, still listed with how and with
//! what it printed last, for the person to look at, until they let it go (`x`) or later ones
//! take its place.

use std::sync::Arc;

use protocol::{JobEnd, JobKind, JobRow};

use super::stop::Stop;
use super::{Jobs, outputs};

/// Jobs that ended still listed, at most: the latest.
const MAX_ENDED: usize = 8;

pub(super) struct Ended {
    id: String,
    name: String,
    kind: JobKind,
    /// Its command.
    detail: String,
    end: JobEnd,
    /// The person let it go.
    stop: Arc<Stop>,
    _known: crate::interrupt::Known,
    _output: outputs::Shown,
}

/// What is known of a job as it ends: its id, name, kind and command.
pub(super) struct Of<'a> {
    pub(super) id: &'a str,
    pub(super) name: &'a str,
    pub(super) kind: JobKind,
    pub(super) detail: &'a str,
}

impl Ended {
    /// Its row in the interface's list.
    pub(super) fn row(&self) -> JobRow {
        JobRow {
            id: self.id.clone(),
            name: self.name.clone(),
            kind: self.kind,
            detail: self.detail.clone(),
            until_at: None,
            next: None,
            events: 0,
            ended: Some(self.end.clone()),
        }
    }
}

/// List job `of` as ended with `end`, what it printed last `printed`: in a session only (in
/// `exec` no one looks), the oldest let go past [`MAX_ENDED`].
pub(super) fn keep(jobs: &Jobs<'_>, of: &Of<'_>, end: JobEnd, printed: String) {
    if !jobs.kit.session {
        return;
    }
    let stop = Arc::new(Stop::default());
    let key = crate::interrupt::job_key(&jobs.owner, of.id);
    let ended = Ended {
        id: of.id.to_string(),
        name: of.name.to_string(),
        kind: of.kind,
        detail: of.detail.to_string(),
        end,
        _known: jobs.known(of.id, &stop),
        stop,
        _output: outputs::register(&key, move || printed.clone()),
    };
    let mut state = jobs.lock();
    state.ended.push(ended);
    let past = state.ended.len().saturating_sub(MAX_ENDED);
    state.ended.drain(..past);
}

/// The ended jobs the person let go leave the list; whether any did.
pub(super) fn let_go(ended: &mut Vec<Ended>) -> bool {
    let listed = ended.len();
    ended.retain(|ended| !ended.stop.asked());
    ended.len() != listed
}
