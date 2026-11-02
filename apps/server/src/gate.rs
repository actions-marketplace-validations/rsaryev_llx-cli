//! How a command or a file change is approved at the terminal, and nothing once
//! the task is stopped. Which effects are asked at all is the mode's (`mode::ModeGate`). The
//! question goes to the terminal itself (`/dev/tty`), so it works while stdin is a pipe, stdout
//! carries JSON and stderr goes to a file. What the question shows came from the model: its
//! control characters are escaped, so it reads as what will run.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::sync::Mutex;

use contracts::gate::{Effect, Gate, Verdict};

use crate::interrupt;
use crate::printable::printable;
use protocol::Approval;

/// Times an unreadable answer is asked again before it counts as no.
const TRIES: usize = 3;
/// How often a waiting question checks whether the task was stopped.
const POLL_MS: libc::c_int = 100;
/// The same, as `select` takes it.
const POLL_US: libc::suseconds_t = 100_000;

/// Refuses everything once the person stopped the task, and asks `inner` until then: nothing
/// more runs after Ctrl-C, not even what was approved while the question was open.
pub(crate) struct Stoppable<'a> {
    pub inner: &'a dyn Gate,
    pub stopped: &'a (dyn Fn() -> bool + Sync),
}

pub(crate) const STOPPED: &str = "the person stopped the task";

/// Hold `person`'s attention for one question (K3): an approval or an `ask`, from the main session
/// or from any agent working beside it, waits on the lock of the person the run has
/// (`Shared::asking`) for the one open to be answered.
pub(crate) fn one_question(person: &Mutex<()>) -> std::sync::MutexGuard<'_, ()> {
    crate::sync::lock(person)
}

impl Gate for Stoppable<'_> {
    fn check(&self, effect: Effect) -> Verdict {
        if (self.stopped)() {
            return Verdict::Deny(STOPPED.into());
        }
        // Stopped while the question was open: a late yes does not count, and a question that
        // Ctrl-C ended reads as the stop it was, not as a silent terminal.
        let verdict = self.inner.check(effect);
        if (self.stopped)() {
            Verdict::Deny(STOPPED.into())
        } else {
            verdict
        }
    }
}

/// Where a question is asked and its answer read.
pub(crate) trait Prompter: Send + Sync {
    /// Show `question` and read one line; `None` when nothing can be read.
    fn ask(&self, question: &str) -> Option<String>;
}

/// The controlling terminal, opened for each question so nothing holds it between them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Tty;

impl Tty {
    /// Whether there is a terminal to ask on. Only the terminal itself counts, not where stdin or
    /// stderr go: a process with none (CI, cron, `nohup`) cannot open it.
    pub(crate) fn available() -> bool {
        Self::open().is_some()
    }

    fn open() -> Option<(File, File)> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .ok()?;
        Some((file.try_clone().ok()?, file))
    }
}

impl Prompter for Tty {
    fn ask(&self, question: &str) -> Option<String> {
        let (input, mut output) = Self::open()?;
        discard_typed_ahead(&input);
        output.write_all(question.as_bytes()).ok()?;
        output.flush().ok()?;
        if !typed(&input, &|| interrupt::PROCESS.stopping()) {
            return None;
        }
        let mut line = String::new();
        match BufReader::new(input).read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line),
        }
    }
}

/// Drop what was typed before the question was shown (the next task, typed while this one ran):
/// it must not answer a question the person never saw.
fn discard_typed_ahead(input: &File) {
    use std::os::fd::AsRawFd;
    // SAFETY: tcflush only discards the terminal's pending input; the descriptor is open.
    unsafe { libc::tcflush(input.as_raw_fd(), libc::TCIFLUSH) };
}

/// Wait until a line can be read from `input`; false when the task was stopped first. The signal
/// handler restarts a blocking read (`SA_RESTART`), so a read alone would keep the question open
/// after Ctrl-C until Enter: waiting in short polls lets the stop end it at once. macOS's `poll`
/// cannot wait on `/dev/tty` (it answers `POLLNVAL` at once), so there `select` waits instead.
fn typed(input: &File, stopped: &dyn Fn() -> bool) -> bool {
    use std::os::fd::AsRawFd;
    let fd = input.as_raw_fd();
    let mut polls = true;
    loop {
        if stopped() {
            return false;
        }
        let ready = match polls {
            true => poll_in(fd),
            false => select_in(fd),
        };
        match ready {
            Readiness::Ready => return true,
            Readiness::Quiet => {}
            Readiness::Unsupported => polls = false,
        }
    }
}

/// What one short wait for input found.
#[derive(Debug, PartialEq, Eq)]
enum Readiness {
    /// Input, a hangup or an error: the read that follows says which.
    Ready,
    /// Nothing within the wait, or a signal ended it.
    Quiet,
    /// This wait cannot be used on the descriptor.
    Unsupported,
}

fn poll_in(fd: libc::c_int) -> Readiness {
    let mut wanted = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: one valid pollfd, for a descriptor that stays open during the call.
    let ready = unsafe { libc::poll(&mut wanted, 1, POLL_MS) };
    if ready > 0 && wanted.revents & libc::POLLNVAL != 0 {
        return Readiness::Unsupported;
    }
    readiness(ready)
}

fn select_in(fd: libc::c_int) -> Readiness {
    // A descriptor past what a set holds cannot be waited on: the read blocks.
    if usize::try_from(fd).map_or(true, |fd| fd >= libc::FD_SETSIZE) {
        return Readiness::Ready;
    }
    let mut timeout = libc::timeval {
        tv_sec: 0,
        tv_usec: POLL_US,
    };
    // SAFETY: an fd_set is plain bits: all zero is a set.
    let mut wanted: libc::fd_set = unsafe { std::mem::zeroed() };
    // SAFETY: FD_ZERO writes only the set, which lives here.
    unsafe { libc::FD_ZERO(&mut wanted) };
    // SAFETY: `fd` is below FD_SETSIZE, checked above.
    unsafe { libc::FD_SET(fd, &mut wanted) };
    let none = std::ptr::null_mut();
    // SAFETY: select reads and writes only the set and the timeout, both of which outlive the
    // call.
    let ready = unsafe { libc::select(fd + 1, &mut wanted, none, none, &mut timeout) };
    readiness(ready)
}

fn readiness(ready: libc::c_int) -> Readiness {
    let interrupted =
        ready < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted;
    match ready != 0 && !interrupted {
        true => Readiness::Ready,
        false => Readiness::Quiet,
    }
}

/// Asks the person whether an effect may happen: yes, always for its tool, or no; `None` when no
/// answer can come. What "always" covers is the mode's (`mode::ModeGate`).
pub(crate) trait Asker: Send + Sync {
    fn approve(&self, effect: &Effect) -> Option<Approval>;
}

/// Asks at the terminal, on `prompter`, one question at a time on the person's lock `asking`.
pub(crate) struct TerminalGate<'a, P: Prompter> {
    pub prompter: P,
    pub asking: &'a Mutex<()>,
}

impl<P: Prompter> Asker for TerminalGate<'_, P> {
    fn approve(&self, effect: &Effect) -> Option<Approval> {
        let tool = printable(effect.tool);
        let question = format!(
            "\n  {tool}: {}\n  allow? [y]es  [n]o (n <why> tells the agent)  [a]lways for {tool} > ",
            printable(effect.summary)
        );
        let _asking = one_question(self.asking);
        for _ in 0..TRIES {
            if let Some(approval) = answer(&self.prompter.ask(&question)?) {
                return Some(approval);
            }
        }
        Some(Approval::No(None))
    }
}

/// The person's line as an answer; `None` when it is none of them. Approval is only the bare
/// word (`y`, `yes`, `a`, `always`): a line that says more ("yes, and then...") was likely meant
/// for something else, and is asked again. A refusal may say why (`n use make`). A bare Enter is
/// no: nothing runs that was not approved.
fn answer(line: &str) -> Option<Approval> {
    let line = line.trim();
    match line.to_ascii_lowercase().as_str() {
        "y" | "yes" => return Some(Approval::Yes),
        "a" | "always" => return Some(Approval::Always),
        _ => {}
    }
    let (word, rest) = line
        .split_once(|c: char| c.is_whitespace() || c == ':' || c == ',')
        .unwrap_or((line, ""));
    let rest = rest.trim_start_matches([':', ',']).trim();
    match word.to_ascii_lowercase().as_str() {
        "" | "n" | "no" => Some(Approval::No((!rest.is_empty()).then(|| rest.to_string()))),
        _ => None,
    }
}

/// What every refusal of the person says, where the tool puts it (after `not allowed: `): the
/// interface knows a declined call by the failure that contains it.
pub(crate) const DECLINED: &str = "the person declined it";

pub(crate) fn declined(why: Option<String>) -> String {
    match why {
        // What they said is the person speaking now, not a tool's error: it comes first.
        Some(why) => format!(
            "{DECLINED} and said: {why}\nThis is the person's instruction to you: follow it \
             before anything else."
        ),
        None => format!("{DECLINED}; do not try the same thing another way"),
    }
}

#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;
