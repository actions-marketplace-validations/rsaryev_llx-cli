//! The log: what each llx process did, said through Rust's `log` facade and kept in one file of
//! the config directory, `logs/llx.log`: the daemon's life, each session's tasks, every call with
//! its arguments and how it ended, the model's turns. `RUST_LOG` chooses what is kept, as
//! env_logger reads it (`warn,llx=debug`, `off`); by default llx's `info` and the rest's warnings.
//! Each line is one event, as env_logger writes it, with the process and the thread. The file is
//! the person's alone (0600: it holds what the tools read), in a directory the workspace never
//! reads, and past `LIMIT` it starts again, the one before kept beside it (`llx.log.1`). A log
//! that cannot be opened is no failure: the run goes on without one.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{LevelFilter, Log, Metadata, Record};

use crate::sync::lock;

/// The file, in the config directory's `logs/`.
const FILE: &str = "llx.log";
/// Past this many bytes the file starts again.
const LIMIT: u64 = 16 * 1024 * 1024;
/// What is kept when `RUST_LOG` says nothing.
const DEFAULT: &str = "warn,llx=info";
/// The longest message kept: a whole file read is the tool result's, not the log's.
const LONGEST: usize = 8 * 1024;

/// Keep this process's log in `config`'s `logs/`, as `RUST_LOG` (else [`DEFAULT`]) says, and
/// log a panic before the hook that was there. Once per process; a second call changes nothing.
pub(crate) fn start(config: &Path) {
    let spec = std::env::var("RUST_LOG").unwrap_or_else(|_| DEFAULT.into());
    let filter = Filter::parse(&spec);
    let max = filter.max();
    if max == LevelFilter::Off {
        return;
    }
    let Ok(file) = Kept::open(&config.join("logs")) else {
        return;
    };
    let logger = Logger {
        filter,
        file: Mutex::new(file),
        pid: std::process::id(),
    };
    // The logger lives as long as the process: `log` keeps a `'static` one.
    if log::set_logger(Box::leak(Box::new(logger))).is_err() {
        return;
    }
    log::set_max_level(max);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!(target: "llx::panic", "{info}");
        previous(info);
    }));
}

/// `RUST_LOG`'s directives: a level for every target, or for a target and what is under it.
#[derive(Debug)]
struct Filter(Vec<(Option<String>, LevelFilter)>);

impl Filter {
    /// `warn,llx=debug,llx::server=trace`; a directive that is not one is left out.
    fn parse(spec: &str) -> Self {
        let directives = spec.split(',').map(str::trim).filter(|d| !d.is_empty());
        Self(
            directives
                .filter_map(|directive| match directive.split_once('=') {
                    Some((target, level)) => Some((Some(target.into()), level.parse().ok()?)),
                    None => match directive.parse() {
                        Ok(level) => Some((None, level)),
                        // A bare target is all of its levels, as in env_logger.
                        Err(_) => Some((Some(directive.into()), LevelFilter::Trace)),
                    },
                })
                .collect(),
        )
    }

    /// The level kept for `target`: its longest directive (`llx::server` under `llx`), else the
    /// bare one, else none.
    fn level(&self, target: &str) -> LevelFilter {
        let under = |of: &str| {
            target == of
                || target
                    .strip_prefix(of)
                    .is_some_and(|rest| rest.starts_with("::"))
        };
        let named = (self.0.iter())
            .filter_map(|(of, level)| of.as_deref().filter(|of| under(of)).map(|of| (of, level)))
            .max_by_key(|(of, _)| of.len());
        let bare = self.0.iter().rev().find(|(of, _)| of.is_none());
        named
            .map(|(_, level)| level)
            .or_else(|| bare.map(|(_, level)| level))
            .copied()
            .unwrap_or(LevelFilter::Off)
    }

    fn max(&self) -> LevelFilter {
        self.0
            .iter()
            .map(|(_, level)| *level)
            .max()
            .unwrap_or(LevelFilter::Off)
    }
}

/// The file and how much of it there is.
struct Kept {
    dir: PathBuf,
    file: File,
    size: u64,
}

impl Kept {
    /// The log in `dir`, made the person's alone; one already past [`LIMIT`] started again.
    fn open(dir: &Path) -> std::io::Result<Self> {
        let mut made = std::fs::DirBuilder::new();
        made.recursive(true);
        std::os::unix::fs::DirBuilderExt::mode(&mut made, 0o700);
        made.create(dir)?;
        if std::fs::metadata(dir.join(FILE)).is_ok_and(|kept| kept.len() > LIMIT) {
            Self::set_aside(dir)?;
        }
        Self::append(dir)
    }

    fn append(dir: &Path) -> std::io::Result<Self> {
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let file = options.open(dir.join(FILE))?;
        let size = file.metadata()?.len();
        Ok(Self {
            dir: dir.to_path_buf(),
            file,
            size,
        })
    }

    /// The file kept as `llx.log.1`, over the one there before.
    fn set_aside(dir: &Path) -> std::io::Result<()> {
        std::fs::rename(dir.join(FILE), dir.join(format!("{FILE}.1")))
    }

    /// `line` written whole; past [`LIMIT`] the file starts again first.
    fn write(&mut self, line: &str) {
        if self.size > LIMIT
            && Self::set_aside(&self.dir).is_ok()
            && let Ok(again) = Self::append(&self.dir)
        {
            *self = again;
        }
        if self.file.write_all(line.as_bytes()).is_ok() {
            self.size += line.len() as u64;
        }
    }
}

struct Logger {
    filter: Filter,
    file: Mutex<Kept>,
    pid: u32,
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.filter.level(metadata.target())
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = line(SystemTime::now(), self.pid, record);
        lock(&self.file).write(&line);
    }

    fn flush(&self) {
        let _ = lock(&self.file).file.flush();
    }
}

/// One event as a line: `[2026-10-27T14:55:01.123Z INFO  llx::session] pid=7 llx-lane: text`.
/// The message's own line breaks are written `\n`, so a line is always one event.
fn line(at: SystemTime, pid: u32, record: &Record) -> String {
    let since = at.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs();
    let stamp = format!(
        "{}T{:02}:{:02}:{:02}.{:03}Z",
        crate::context::date(secs),
        secs / 3600 % 24,
        secs / 60 % 60,
        secs % 60,
        since.subsec_millis()
    );
    let thread = std::thread::current();
    let thread = thread.name().unwrap_or("-");
    let mut said = record.args().to_string().replace('\n', "\\n");
    if said.len() > LONGEST {
        said.truncate(crate::context::cut(&said, LONGEST).len());
        said.push_str(" …");
    }
    format!(
        "[{stamp} {:<5} {}] pid={pid} {thread}: {said}\n",
        record.level(),
        record.target()
    )
}

#[cfg(test)]
#[path = "logs_tests.rs"]
mod tests;
