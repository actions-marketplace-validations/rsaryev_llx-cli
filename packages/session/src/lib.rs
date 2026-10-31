//! A history store: each session one JSON file in a folder the app chooses, written whole to a
//! temporary file and renamed, so a crash never leaves half a session.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use contracts::history::{History, HistoryError, Session};

/// Unix seconds now.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Sessions as files in `dir`.
pub struct Sessions {
    pub dir: PathBuf,
}

impl History for Sessions {
    fn start(&self) -> Session {
        let mut random = [0u8; 2];
        let _ = getrandom::fill(&mut random);
        let created = now();
        Session {
            id: format!("{created}-{:02x}{:02x}", random[0], random[1]),
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
        let mut sessions: Vec<Session> = entries
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|e| e == "json"))
            .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
            .filter_map(|text| serde_json::from_str::<Session>(&text).ok())
            .collect();
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated));
        sessions
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
