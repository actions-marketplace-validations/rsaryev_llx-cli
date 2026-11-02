//! A history store: each session one JSON file in a folder the app chooses, written whole to a
//! temporary file and renamed, so a crash never leaves half a session.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use contracts::history::{History, HistoryError, Session};

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

    fn save(&self, session: &Session) -> Result<(), HistoryError> {
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(&self.dir)?;
            let path = self.dir.join(format!("{}.json", session.id));
            let partial = self.dir.join(format!(".{}.json.part", session.id));
            std::fs::write(&partial, serde_json::to_vec(session)?)?;
            std::fs::rename(&partial, &path)
        };
        write().map_err(|error| HistoryError::Write(error.to_string()))
    }

    fn load(&self, id: Option<&str>) -> Result<Session, HistoryError> {
        let Some(id) = id else {
            // An agent's session goes on only through its parent; `list` has read it already.
            return (self.list().into_iter())
                .find(|session| !session.is_agent())
                .ok_or(HistoryError::Empty);
        };
        let text =
            std::fs::read_to_string(self.dir.join(format!("{id}.json"))).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    HistoryError::NotFound(id.to_string())
                } else {
                    HistoryError::Unreadable {
                        id: id.to_string(),
                        why: error.to_string(),
                    }
                }
            })?;
        serde_json::from_str(&text).map_err(|error| HistoryError::Corrupt {
            id: id.to_string(),
            why: error.to_string(),
        })
    }

    fn list(&self) -> Vec<Session> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return vec![];
        };
        let paths: Vec<PathBuf> = (entries.flatten())
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|e| e == "json"))
            .collect();
        let mut sessions = read_all(&paths);
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated));
        sessions
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
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .filter_map(|text| serde_json::from_str::<Session>(&text).ok())
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
