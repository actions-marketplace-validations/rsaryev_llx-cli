//! The real `llx` binary in a pseudo-terminal, its screen read as a person sees it (`vt100`): a
//! test types, presses keys and waits for what the screen shows, as one uses llx. Everything it
//! touches is its own: a home, a config, sessions, a workspace, and the scripted model in place of
//! the network.

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::eventually;
use super::home::{FAKE_KEY, Home, isolate, own_terminal, text};
use super::model::Model;

const COLS: u16 = 110;
const ROWS: u16 = 40;

/// Keys as a terminal sends them (only those the flows press).
pub(crate) mod key {
    pub(crate) const ENTER: &str = "\r";
    pub(crate) const ESC: &str = "\x1b";
    pub(crate) const DOWN: &str = "\x1b[B";
    pub(crate) const LEFT: &str = "\x1b[D";
    pub(crate) const CTRL_C: &str = "\x03";
    pub(crate) const CTRL_D: &str = "\x04";
    pub(crate) const CTRL_Z: &str = "\x1a";
}

/// A session of `llx` at a terminal.
pub(crate) struct Term {
    child: Child,
    input: File,
    screen: Arc<Mutex<vt100::Parser>>,
    /// Every byte llx wrote, for what the screen does not keep (a query to the terminal).
    raw: Arc<Mutex<Vec<u8>>>,
    /// The home, shared by the terminals beside this one: its daemon stops, and it goes, with the
    /// last of them (after its child, as fields drop after [`Drop::drop`]).
    home: Arc<Home>,
}

impl Term {
    /// `llx` with `args`, in a fresh home and workspace, thinking with `model`.
    pub(crate) fn start(model: &Model, args: &[&str]) -> Self {
        let term = Self::spawn(model, args, &[]);
        term.wait_for("❯", Duration::from_secs(10));
        term
    }

    /// `llx resume ID`, where session `id` was saved in the workspace with the tasks `turns`
    /// (their JSON array).
    pub(crate) fn resuming(model: &Model, id: &str, turns: &str) -> Self {
        Self::resuming_from(model, id, turns, None)
    }

    /// [`Term::resuming`], the session saved as having run in `ran_in` (else the workspace).
    pub(crate) fn resuming_from(
        model: &Model,
        id: &str,
        turns: &str,
        ran_in: Option<&str>,
    ) -> Self {
        let home = place();
        let ran_in = ran_in.map_or_else(|| workspace(&home).display().to_string(), str::to_string);
        let turns = serde_json::from_str(turns).unwrap();
        home.save_turns(id, &ran_in, 1700000000, &turns);
        let term = Self::spawn_at(home, model, &["resume", id], &[]);
        term.wait_for("❯", Duration::from_secs(10));
        term
    }

    /// `llx` with `args` and the variables `env`, not waited on.
    pub(crate) fn spawn(model: &Model, args: &[&str], env: &[(&str, &str)]) -> Self {
        Self::spawn_at(place(), model, args, env)
    }

    /// Another terminal's `llx` with `args`, in this one's home and workspace, at its prompt.
    pub(crate) fn beside(&self, model: &Model, args: &[&str]) -> Self {
        self.beside_with(model, args, &[])
    }

    /// [`Term::beside`], with the variables `env` set in that terminal alone.
    pub(crate) fn beside_with(&self, model: &Model, args: &[&str], env: &[(&str, &str)]) -> Self {
        let term = Self::spawn_at(Arc::clone(&self.home), model, args, env);
        term.wait_for("❯", Duration::from_secs(10));
        term
    }

    /// [`Term::beside`], not waited on: for an `llx` that may end before its prompt shows.
    pub(crate) fn next_to(&self, model: &Model, args: &[&str]) -> Self {
        Self::spawn_at(Arc::clone(&self.home), model, args, &[])
    }

    /// `llx args…` in this session's home, run to its end: what it printed, and its exit code.
    pub(crate) fn llx(&self, args: &[&str]) -> (String, i32) {
        let output = self.home.run(args);
        let said = text(&output.stdout) + &text(&output.stderr);
        (said, output.status.code().unwrap_or(-1))
    }

    /// An interactive shell with job control, at its `$ ` prompt: what suspends and continues
    /// `llx`. (The child of `Term::start` leads a session of its own, an orphaned process group,
    /// where a stop signal is discarded.) `llx` is in `$LLX`.
    pub(crate) fn in_shell(model: &Model) -> Self {
        let env = [
            ("PS1", "$ "),
            ("HISTFILE", "/dev/null"),
            ("LLX", env!("CARGO_BIN_EXE_llx")),
        ];
        let term = Self::spawn_program("sh", place(), model, &["-i"], &env);
        term.wait_for("$ ", Duration::from_secs(10));
        term
    }

    fn spawn_at(home: Arc<Home>, model: &Model, args: &[&str], env: &[(&str, &str)]) -> Self {
        Self::spawn_program(env!("CARGO_BIN_EXE_llx"), home, model, args, env)
    }

    fn spawn_program(
        program: &str,
        home: Arc<Home>,
        model: &Model,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Self {
        let (master, slave) = pty();
        let size = libc::winsize {
            ws_row: ROWS,
            ws_col: COLS,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: TIOCSWINSZ reads `size`, which outlives the call; the descriptor is ours.
        unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ, &size) };
        let file = |fd: &OwnedFd| Stdio::from(fd.try_clone().unwrap());
        let mut command = Command::new(program);
        own_terminal(&mut command);
        isolate(&mut command, home.path());
        let child = command
            .args(args)
            .current_dir(workspace(&home))
            .env("TERM", "xterm-256color")
            .env("OPENROUTER_API_KEY", FAKE_KEY)
            .env("OPENROUTER_BASE_URL", &model.base)
            .env("LLX_PROVIDER", "openrouter")
            .env("LLX_MODEL", "anthropic/claude-haiku-5.5")
            .envs(env.iter().copied())
            .stdin(file(&slave))
            .stdout(file(&slave))
            .stderr(file(&slave))
            .spawn()
            .unwrap();
        drop(slave);
        let screen = Arc::new(Mutex::new(vt100::Parser::new(ROWS, COLS, 0)));
        let reading = Arc::clone(&screen);
        let raw = Arc::new(Mutex::new(vec![]));
        let written = Arc::clone(&raw);
        let mut output = File::from(master.try_clone().unwrap());
        let reader = std::thread::Builder::new().name("term-reader".into());
        reader
            .spawn(move || {
                let mut buffer = [0u8; 65536];
                while let Ok(n) = output.read(&mut buffer) {
                    if n == 0 {
                        break;
                    }
                    reading.lock().unwrap().process(&buffer[..n]);
                    written.lock().unwrap().extend_from_slice(&buffer[..n]);
                }
            })
            .unwrap();
        Self {
            child,
            input: File::from(master),
            screen,
            raw,
            home,
        }
    }

    /// Every byte llx wrote so far.
    pub(crate) fn raw(&self) -> Vec<u8> {
        self.raw.lock().unwrap().clone()
    }

    /// What the screen shows now, row by row.
    pub(crate) fn screen(&self) -> String {
        self.screen.lock().unwrap().screen().contents()
    }

    /// The row the cursor is on: the composer's while the prompt waits, never a row of the
    /// history above it.
    pub(crate) fn cursor_row(&self) -> String {
        let parser = self.screen.lock().unwrap();
        let (row, _) = parser.screen().cursor_position();
        let row = parser.screen().rows(0, COLS).nth(row as usize);
        row.unwrap_or_default().trim_end().to_string()
    }

    /// Wait until the cursor's row reads `row`; the screen, or a panic that prints it.
    pub(crate) fn wait_for_cursor_row(&self, row: &str, timeout: Duration) -> String {
        let read = || (self.cursor_row() == row).then(|| self.screen());
        eventually(timeout, Duration::from_millis(50), read).unwrap_or_else(|| {
            let (at, screen) = (self.cursor_row(), self.screen());
            panic!("the cursor's row never read {row:?} (it reads {at:?}):\n{screen}")
        })
    }

    /// Press keys, or type text, as a terminal sends them, then wait 60 ms: keys sent back to
    /// back can arrive in one read and parse as one key (two Escs as Alt+Esc).
    #[expect(
        clippy::disallowed_methods,
        reason = "a pause after the keys: sent back to back they arrive as one"
    )]
    pub(crate) fn press(&mut self, keys: &str) -> &mut Self {
        self.input.write_all(keys.as_bytes()).unwrap();
        self.input.flush().unwrap();
        std::thread::sleep(Duration::from_millis(60));
        self
    }

    /// Type `text` a character at a time, as a person does.
    #[expect(
        clippy::disallowed_methods,
        reason = "a pause between keys, as a person's: text followed at once by Enter is taken for a paste"
    )]
    pub(crate) fn type_text(&mut self, text: &str) -> &mut Self {
        for c in text.chars() {
            self.input.write_all(c.to_string().as_bytes()).unwrap();
            std::thread::sleep(Duration::from_millis(4));
        }
        std::thread::sleep(Duration::from_millis(60));
        self
    }

    /// Wait until the screen shows `needle`; the screen, or a panic that prints it.
    pub(crate) fn wait_for(&self, needle: &str, timeout: Duration) -> String {
        self.wait_for_any(&[needle], timeout).1
    }

    /// Wait until the screen shows one of `needles`; the first one shown and the screen, or a
    /// panic that prints it.
    pub(crate) fn wait_for_any<'a>(
        &self,
        needles: &[&'a str],
        timeout: Duration,
    ) -> (&'a str, String) {
        let shown = || {
            let screen = self.screen();
            let found = needles.iter().find(|needle| screen.contains(**needle))?;
            Some((*found, screen))
        };
        eventually(timeout, Duration::from_millis(50), shown).unwrap_or_else(|| {
            let screen = self.screen();
            panic!("the screen never showed any of {needles:?}:\n{screen}")
        })
    }

    /// Wait up to `timeout` for llx to exit; its exit code, `None` if it did not.
    pub(crate) fn exited(&mut self, timeout: Duration) -> Option<i32> {
        let status = || self.child.try_wait().ok().flatten();
        let status = eventually(timeout, Duration::from_millis(20), status)?;
        Some(status.code().unwrap_or(-1))
    }

    /// The sessions saved so far, as JSON.
    pub(crate) fn saved(&self) -> Vec<serde_json::Value> {
        let files = std::fs::read_dir(self.home.path().join("sessions"))
            .into_iter()
            .flatten();
        (files.flatten())
            .filter_map(|file| std::fs::read(file.path()).ok())
            .filter_map(|bytes| serde_json::from_slice(&bytes).ok())
            .collect()
    }

    /// A file of the workspace the session runs in.
    pub(crate) fn file(&self, name: &str) -> PathBuf {
        workspace(&self.home).join(name)
    }

    /// The process of this terminal's `llx`.
    #[cfg_attr(
        not(target_os = "linux"),
        expect(dead_code, reason = "read in /proc, on Linux")
    )]
    pub(crate) fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Send llx `signal`, as a closed window (SIGHUP) or `kill` (SIGTERM) does.
    pub(crate) fn signal(&self, signal: libc::c_int) {
        let pid = libc::pid_t::try_from(self.child.id()).unwrap();
        // SAFETY: kill only sends a signal, to the child this harness started.
        unsafe { libc::kill(pid, signal) };
    }

    /// Whether the terminal reads lines again, echoed: not left raw.
    pub(crate) fn cooked(&self) -> bool {
        use std::os::fd::AsFd;
        cooked(self.input.as_fd())
    }

    /// What the daemon of this home wrote to its log so far.
    pub(crate) fn daemon_log(&self) -> String {
        let log = self.home.path().join("config/llx/daemon.log");
        std::fs::read_to_string(log).unwrap_or_default()
    }
}

/// A fresh home, with the workspace in it.
fn place() -> Arc<Home> {
    let home = Home::new();
    std::fs::create_dir_all(workspace(&home)).unwrap();
    Arc::new(home)
}

/// The workspace in `home` the sessions run in.
fn workspace(home: &Home) -> PathBuf {
    home.path().join("work")
}

impl Drop for Term {
    /// The session is ended as a person ends it, so that it exits on its own (an instrumented
    /// binary writes its coverage then); one that does not within 3 s is killed.
    fn drop(&mut self) {
        // Ctrl-C stops a task or clears the prompt; Ctrl-D on an empty prompt then quits, and
        // where work still runs, the question it asks is answered "Exit and stop tasks".
        let _ = self.input.write_all(b"\x03");
        let screen = Arc::clone(&self.screen);
        let _ = eventually(Duration::from_secs(3), Duration::from_millis(100), || {
            if let Ok(Some(_)) = self.child.try_wait() {
                return Some(());
            }
            let shown = screen.lock().unwrap().screen().contents();
            let quitting = shown.contains("1. Exit and stop tasks");
            let _ = self.input.write_all(if quitting { b"1" } else { b"\x04" });
            None
        });
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Whether the terminal `fd` reads lines again, echoed: not left raw.
pub(crate) fn cooked(fd: BorrowedFd<'_>) -> bool {
    // SAFETY: an all-zero termios is a valid value for tcgetattr to fill.
    let mut modes: libc::termios = unsafe { std::mem::zeroed() };
    // SAFETY: tcgetattr writes `modes`, which outlives the call; the descriptor is open.
    let read = unsafe { libc::tcgetattr(fd.as_raw_fd(), &mut modes) };
    assert_eq!(read, 0, "{}", std::io::Error::last_os_error());
    modes.c_lflag & (libc::ICANON | libc::ECHO) == (libc::ICANON | libc::ECHO)
}

/// A pseudo-terminal: its master, then its slave.
pub(crate) fn pty() -> (OwnedFd, OwnedFd) {
    let (mut master, mut slave) = (0, 0);
    // SAFETY: openpty fills both descriptors, which become ours.
    let opened = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(opened, 0, "openpty");
    // Close-on-exec, so no child spawned meanwhile keeps the master open (its slave would then
    // never read the end); the child's own stdin is a copy, which keeps it open.
    for fd in [master, slave] {
        // SAFETY: fcntl sets a flag on a descriptor just opened.
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
    }
    // SAFETY: a descriptor just opened, owned by nobody else.
    let master = unsafe { OwnedFd::from_raw_fd(master) };
    // SAFETY: a descriptor just opened, owned by nobody else.
    let slave = unsafe { OwnedFd::from_raw_fd(slave) };
    (master, slave)
}
