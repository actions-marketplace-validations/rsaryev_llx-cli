//! A history store: each session one JSON Lines file in a folder the app chooses, `<id>.jsonl`,
//! appended to by its one writer as it goes (`journal`, ARCHITECTURE.md K8).

mod journal;

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use contracts::history::{History, HistoryError, Record, Session};

/// Unix seconds now.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The last part of the ids this process gives: drawn at random once, then counted up, so no two
/// sessions it starts in one second share an id (agents spawned together), and another process's,
/// drawn apart, seldom meet them.
fn serial() -> u16 {
    static NEXT: OnceLock<AtomicU16> = OnceLock::new();
    let next = NEXT.get_or_init(|| {
        let mut random = [0u8; 2];
        let _ = getrandom::fill(&mut random);
        AtomicU16::new(u16::from_be_bytes(random))
    });
    next.fetch_add(1, Ordering::Relaxed)
}

/// Sessions as files in `dir`.
pub struct Sessions {
    pub dir: PathBuf,
}

impl History for Sessions {
    fn start(&self) -> Session {
        let created = now();
        Session {
            id: format!("{created}-{:04x}", serial()),
            created,
            updated: created,
            ..Session::default()
        }
    }

    fn load(&self, id: &str) -> Result<Session, HistoryError> {
        let text = std::fs::read_to_string(self.file(id)).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                HistoryError::NotFound(id.to_string())
            } else {
                HistoryError::Unreadable {
                    id: id.to_string(),
                    why: error.to_string(),
                }
            }
        })?;
        journal::read(id, &text)
    }

    fn list(&self) -> Vec<Session> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return vec![];
        };
        let paths: Vec<PathBuf> = (entries.flatten())
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|e| e == "jsonl"))
            .collect();
        let mut sessions = read_all(&paths);
        // One with no task (a crash left it so) has nothing to go on with.
        sessions.retain(|session| !session.turns.is_empty());
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated));
        sessions
    }

    fn hold(&self, session: &Session) -> Result<Box<dyn Record>, HistoryError> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|error| HistoryError::Write(error.to_string()))?;
        let journal = journal::Journal::hold(self.file(&session.id), session)?;
        Ok(Box::new(journal))
    }
}

impl Sessions {
    /// Where session `id` is kept.
    fn file(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.jsonl"))
    }
}

/// Files read by one thread at least: below it, a thread costs more than it saves.
const PER_THREAD: usize = 64;
/// Threads that read at most: past them, the disk is the limit.
const THREADS: usize = 8;

/// The sessions of `paths` that read as sessions, read by a few threads at once: opening and
/// parsing are each about half of a list's time, and a list of thousands is read whole each time
/// the person opens it.
fn read_all(paths: &[PathBuf]) -> Vec<Session> {
    let read = |paths: &[PathBuf]| -> Vec<Session> {
        (paths.iter())
            .filter_map(|path| {
                let id = path.file_stem()?.to_str()?;
                journal::read(id, &std::fs::read_to_string(path).ok()?).ok()
            })
            .collect()
    };
    let threads = (paths.len() / PER_THREAD).clamp(1, THREADS);
    if threads == 1 {
        return read(paths);
    }
    let chunk = paths.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let read = &read;
        // Every thread started before the first is joined, so they read at once.
        let mut started = Vec::with_capacity(threads);
        for (n, part) in paths.chunks(chunk).enumerate() {
            let spawned = std::thread::Builder::new()
                .name(format!("sessions-{n}"))
                .spawn_scoped(scope, move || read(part));
            started.push((part, spawned));
        }
        let mut sessions = Vec::with_capacity(paths.len());
        for (part, spawned) in started {
            match spawned {
                Ok(handle) => sessions.extend(handle.join().unwrap_or_default()),
                // A thread the OS refused: its part is read here.
                Err(_) => sessions.extend(read(part)),
            }
        }
        sessions
    })
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
