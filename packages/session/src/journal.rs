//! A session's file, as JSON Lines: its header, then each task and each fact as it came, every
//! one appended (ARCHITECTURE.md K8). Its one writer holds the file's lock for as long as it
//! lives, and the operating system lets the lock go when the process ends, however it ends.

use std::borrow::Cow;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::os::unix::fs::FileExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use contracts::history::{HistoryError, Record, Session, Turn};
use serde::{Deserialize, Serialize};

/// One line of a session's file: a task and a title borrowed as they are written, owned as they
/// are read.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Line<'a> {
    /// The first line: which session this is, and where it ran.
    Session {
        id: String,
        created: u64,
        workspace: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        name: String,
    },
    /// A task that ended.
    Turn(Cow<'a, Turn>),
    /// The session's title, as it was made.
    Title { title: Cow<'a, str> },
}

/// Session `id` read back from its file's `text`: the header first, then each line in order. A
/// line that does not read as one is left out: only the last can be, cut short by a crash.
pub(crate) fn read(id: &str, text: &str) -> Result<Session, HistoryError> {
    let corrupt = |why: String| HistoryError::Corrupt {
        id: id.to_string(),
        why,
    };
    let mut lines = text.lines();
    let first = serde_json::from_str(lines.next().unwrap_or_default());
    let Line::Session {
        id,
        created,
        workspace,
        parent,
        name,
    } = first.map_err(|error| corrupt(error.to_string()))?
    else {
        return Err(corrupt("its first line is not its header".into()));
    };
    let mut session = Session {
        id,
        created,
        updated: created,
        workspace,
        parent,
        name,
        ..Session::default()
    };
    for line in lines.filter_map(|line| serde_json::from_str(line).ok()) {
        match line {
            // A header past the first line was never written by a writer: nothing to keep.
            Line::Session { .. } => {}
            Line::Turn(turn) => session.record(turn.into_owned()),
            Line::Title { title } => session.title = title.into_owned(),
        }
    }
    Ok(session)
}

/// A held session's writer: its file open for appending, and locked.
pub(crate) struct Journal {
    file: File,
    path: PathBuf,
    /// The file was started by this writer, so a session it leaves with no task is removed:
    /// there is nothing in it to go on with.
    fresh: bool,
    /// A task was written.
    turned: AtomicBool,
}

impl Journal {
    /// The writer of `session`, at `path`: refused while another holds it, in this process or
    /// another. A new file starts with the header; one a crash cut short is given its line's end,
    /// so the next line is whole.
    pub(crate) fn hold(path: PathBuf, session: &Session) -> Result<Self, HistoryError> {
        let file = (OpenOptions::new().read(true).append(true).create(true))
            .open(&path)
            .map_err(|error| written(&error))?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(HistoryError::Held(session.id.clone())),
            Err(TryLockError::Error(error)) => return Err(written(&error)),
        }
        let length = file.metadata().map_err(|error| written(&error))?.len();
        let journal = Self {
            file,
            path,
            fresh: length == 0,
            turned: AtomicBool::new(false),
        };
        if journal.fresh {
            journal.append(&Line::Session {
                id: session.id.clone(),
                created: session.created,
                workspace: session.workspace.clone(),
                parent: session.parent.clone(),
                name: session.name.clone(),
            })?;
        } else {
            let mut last = [0u8];
            (journal.file.read_exact_at(&mut last, length - 1)).map_err(|error| written(&error))?;
            if last != *b"\n" {
                (&journal.file)
                    .write_all(b"\n")
                    .map_err(|error| written(&error))?;
            }
        }
        Ok(journal)
    }

    /// `line` at the file's end, in one write.
    fn append(&self, line: &Line) -> Result<(), HistoryError> {
        let mut bytes =
            serde_json::to_vec(line).map_err(|error| HistoryError::Write(error.to_string()))?;
        bytes.push(b'\n');
        (&self.file)
            .write_all(&bytes)
            .map_err(|error| written(&error))
    }
}

impl Record for Journal {
    fn turn(&self, turn: &Turn) -> Result<(), HistoryError> {
        self.append(&Line::Turn(Cow::Borrowed(turn)))?;
        self.turned.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn title(&self, title: &str) -> Result<(), HistoryError> {
        self.append(&Line::Title {
            title: Cow::Borrowed(title),
        })
    }
}

impl Drop for Journal {
    fn drop(&mut self) {
        // Removed while still locked: no other writer can have it meanwhile.
        if self.fresh && !self.turned.load(Ordering::SeqCst) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn written(error: &std::io::Error) -> HistoryError {
    HistoryError::Write(error.to_string())
}
