//! The terminal itself: its modes, the keys read from it, and the person's editor, which has it
//! for a while.

use std::io::{self, Write};
use std::sync::mpsc::Sender;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crossterm::event::{Event as Terminal, KeyEventKind};

use super::event::Input;

/// Bracketed paste on, and keys reported so Shift+Enter differs from Enter where the terminal can.
pub(super) const ENTER: &str = "\x1b[?2004h\x1b[>1u";
const LEAVE: &str = "\x1b[<1u\x1b[?2004l\x1b[?25h";

/// Where frames are written: the terminal through a descriptor of its own, not std's stdout,
/// which is line-buffered: it writes a frame up to its last line break and keeps the rest (the
/// last row, the cursor's place, the update's end) for a second write at the flush. A frame is
/// one buffer (`Screen`), so it goes out in one write.
pub(super) fn out() -> Box<dyn Write> {
    if let Ok(own) = unbuffered(io::stdout()) {
        return Box::new(own);
    }
    Box::new(io::stdout())
}

/// A writer to `fd` that keeps nothing back.
fn unbuffered(fd: impl std::os::fd::AsFd) -> io::Result<std::fs::File> {
    Ok(fd.as_fd().try_clone_to_owned()?.into())
}

/// The terminal as it was: cooked, the cursor shown, nothing more reported.
pub fn restore() {
    let _ = crossterm::terminal::disable_raw_mode();
    let mut out = io::stdout();
    let _ = out.write_all(LEAVE.as_bytes()).and_then(|()| out.flush());
}

/// The terminal's modes before the interface made it raw, for a signal handler to put back.
static SAVED: std::sync::OnceLock<libc::termios> = std::sync::OnceLock::new();

/// How the process ends at once, given by the app; called from the signal handler, so it must
/// be async-signal-safe.
static QUIT: std::sync::OnceLock<fn() -> !> = std::sync::OnceLock::new();

/// SIGTERM and SIGHUP end the process with `quit` once the terminal is given back: its modes as
/// they were, the cursor shown, nothing more reported (the client has no other handler; in
/// process, `interrupt::install` replaces SIGTERM's with stopping the task). Called before the
/// terminal is made raw, whose modes it keeps.
pub(super) fn on_signals(quit: fn() -> !) {
    extern "C" fn given_back(_: libc::c_int) {
        if let Some(modes) = SAVED.get() {
            // SAFETY: tcsetattr is async-signal-safe and only reads `modes`, which is static.
            unsafe { libc::tcsetattr(0, libc::TCSANOW, modes) };
        }
        // SAFETY: write is async-signal-safe, and LEAVE is static.
        unsafe { libc::write(1, LEAVE.as_ptr().cast(), LEAVE.len()) };
        if let Some(quit) = QUIT.get() {
            quit();
        }
        // SAFETY: _exit is async-signal-safe. Not reached: `quit` is set before the handler is.
        unsafe { libc::_exit(1) };
    }
    let _ = QUIT.set(quit);
    // SAFETY: isatty only looks at the descriptor.
    if unsafe { libc::isatty(0) } == 1 {
        // SAFETY: a termios is integers and arrays of them: all zero is one, for tcgetattr to fill.
        let mut modes: libc::termios = unsafe { std::mem::zeroed() };
        // SAFETY: tcgetattr writes only `modes`, which outlives the call.
        if unsafe { libc::tcgetattr(0, &mut modes) } == 0 {
            let _ = SAVED.set(modes);
        }
    }
    // SAFETY: a sigaction is integers, a mask and a handler's address: all zero is one, with no
    // handler, until it is set below.
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = given_back as extern "C" fn(libc::c_int) as libc::sighandler_t;
    // SAFETY: sigemptyset writes only the mask, which lives here.
    unsafe { libc::sigemptyset(&mut action.sa_mask) };
    for signal in [libc::SIGTERM, libc::SIGHUP] {
        // SAFETY: the action is whole and outlives the call, which copies it; the handler does
        // only what a signal handler may (tcsetattr, write, kill, _exit).
        unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) };
    }
}

/// Who reads the terminal's keys: the reader thread, or for a while the person's editor.
static KEYS: Keys = Keys::new();

/// How long the editor waits for the reader to give way: it does at once, unless it reads no more.
const GIVE_WAY: Duration = Duration::from_secs(1);

/// The keys' turn, and the wake for whoever waits for it to change.
struct Keys {
    turn: Mutex<Turn>,
    changed: Condvar,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Turn {
    /// The reader's: it waits on the terminal.
    Reader,
    /// The editor wants the terminal: the reader gives way once out of its read.
    Asked,
    /// The editor's: the reader waits until it is given back.
    Editor,
}

impl Keys {
    const fn new() -> Self {
        Self {
            turn: Mutex::new(Turn::Reader),
            changed: Condvar::new(),
        }
    }

    fn turn(&self) -> MutexGuard<'_, Turn> {
        self.turn.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The reader's, between two reads: while the editor wants the terminal or has it, nothing
    /// is read until it is given back.
    fn between(&self) {
        let mut turn = self.turn();
        if *turn == Turn::Asked {
            *turn = Turn::Editor;
            self.changed.notify_all();
        }
        let given = self.changed.wait_while(turn, |turn| *turn != Turn::Reader);
        drop(given.unwrap_or_else(PoisonError::into_inner));
    }

    /// The editor's: the terminal, once the reader gave way; `wake` gets it out of its read.
    fn take(&self, wake: impl FnOnce()) {
        *self.turn() = Turn::Asked;
        wake();
        let asked = self.turn();
        let given = (self.changed).wait_timeout_while(asked, GIVE_WAY, |t| *t == Turn::Asked);
        drop(given.unwrap_or_else(PoisonError::into_inner));
    }

    fn give_back(&self) {
        *self.turn() = Turn::Reader;
        self.changed.notify_all();
    }
}

/// The terminal's keys, pastes and sizes, read as they come, to `events`.
pub(super) fn read_keys(events: &Sender<Input>) {
    loop {
        KEYS.between();
        // The reader sleeps in the read until an event comes (R9); the editor's turn wakes it
        // with a resize (`wake_reader`).
        let event = match crossterm::event::read() {
            Ok(Terminal::Key(key)) if key.kind != KeyEventKind::Release => Input::Key(key),
            Ok(Terminal::Paste(text)) => Input::Paste(text),
            Ok(Terminal::Resize(width, height)) => Input::Resize(width, height),
            Ok(_) => continue,
            Err(_) => return,
        };
        if events.send(event).is_err() {
            return;
        }
    }
}

/// Gets the reader out of its read: crossterm's reader also wakes on SIGWINCH, and reads it as a
/// resize, of the size the terminal has (which changes nothing).
fn wake_reader() {
    // SAFETY: raise only sends this thread a signal; SIGWINCH is crossterm's to handle, and
    // ignored where nothing handles it.
    unsafe { libc::raise(libc::SIGWINCH) };
}

/// `during` with the terminal handed over: the reader reads no key meanwhile, the terminal is as
/// it was before llx took it, and the interface's modes are set again after, with the keys given
/// back. Its result.
fn handed_over<T>(during: impl FnOnce() -> T) -> T {
    KEYS.take(wake_reader);
    restore();
    let result = during();
    let _ = crossterm::terminal::enable_raw_mode();
    let mut out = io::stdout();
    let _ = out.write_all(ENTER.as_bytes()).and_then(|()| out.flush());
    KEYS.give_back();
    result
}

/// The terminal given back and the process stopped (Ctrl+Z), as vim and less do; it returns once
/// the shell continues it (`fg`), the terminal the interface's again. SIGTSTP is raised on this
/// thread and stops every thread of the process: the server's, in process, with it.
pub(super) fn suspend() {
    handed_over(|| {
        // SAFETY: raise only sends this thread a signal; SIGTSTP is not handled here, so its
        // default action stops the process, and the call returns after SIGCONT.
        unsafe { libc::raise(libc::SIGTSTP) };
    });
}

/// The composer's `text` written in `$VISUAL` or `$EDITOR` (else `vi`), the terminal handed over
/// meanwhile; the text as saved, or as it was when the editor could not run.
#[expect(
    clippy::disallowed_methods,
    reason = "the person's own editor, with no deadline by design: it ends when they leave it"
)]
pub(super) fn editor(text: &str) -> String {
    let Ok(path) = draft(&std::env::temp_dir(), text) else {
        return text.to_string();
    };
    let ran = handed_over(|| {
        let command = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "vi".into());
        std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("{command} \"$0\""))
            .arg(&path)
            .status()
    });
    let edited = ran
        .ok()
        .filter(|status| status.success())
        .and_then(|_| std::fs::read_to_string(&path).ok());
    let _ = std::fs::remove_file(&path);
    edited.unwrap_or_else(|| text.to_string())
}

/// The name of the editor's draft, at its `attempt`.
fn draft_name(attempt: u32) -> String {
    format!("llx-prompt-{}-{attempt}.md", std::process::id())
}

/// `text` in a draft file in `dir`, for the editor; its path. The file is made new (`O_EXCL`,
/// which follows no link) and 0600: in a shared `/tmp` a name the pid gives away may already be
/// taken, by a link to the person's own file; then the next name is tried.
fn draft(dir: &std::path::Path, text: &str) -> io::Result<std::path::PathBuf> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut taken = io::Error::from(io::ErrorKind::AlreadyExists);
    for attempt in 1..=8 {
        let path = dir.join(draft_name(attempt));
        let made = (std::fs::OpenOptions::new().write(true).create_new(true))
            .mode(0o600)
            .open(&path);
        match made {
            Ok(mut file) => return file.write_all(text.as_bytes()).map(|()| path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => taken = error,
            Err(error) => return Err(error),
        }
    }
    Err(taken)
}

/// `lines` printed under the history, dim, once the terminal is given back.
pub(super) fn say(lines: &[ratatui::text::Line<'static>]) {
    if lines.is_empty() {
        return;
    }
    let colour = super::style::colour_on(&io::stdout());
    let text = super::style::rows(lines, colour);
    let mut out = io::stdout();
    let _ = out.write_all(text.as_bytes()).and_then(|()| out.flush());
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
