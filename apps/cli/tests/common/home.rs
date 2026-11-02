//! A home of its own for each test, and `llx` run in it: no key in the environment, stdin closed,
//! nothing of the person's read.

use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};

use super::model::Model;

/// An OpenRouter key's shape, never a real one.
pub(crate) const FAKE_KEY: &str = "sk-or-v1-0000000000000000000000000000wxyz";

/// A fresh, empty home for one test, removed when it ends.
pub(crate) struct Home(PathBuf);

impl Home {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "llx-e2e-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    /// `llx args…` with this home, no key in the environment and stdin closed.
    pub(crate) fn llx(&self, args: &[&str]) -> Command {
        let mut command = self.command(env!("CARGO_BIN_EXE_llx"));
        command.args(args);
        command
    }

    /// `program` as [`Home::llx`] runs the binary: in this home, stdin closed.
    pub(crate) fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        isolate(&mut command, &self.0);
        command.stdin(Stdio::null());
        command
    }

    /// `llx args…` run to its end.
    pub(crate) fn run(&self, args: &[&str]) -> Output {
        output(&mut self.llx(args))
    }

    /// `llx args…` run to its end with [`FAKE_KEY`] in the environment.
    pub(crate) fn run_with_key(&self, args: &[&str]) -> Output {
        output(self.llx(args).env("OPENROUTER_API_KEY", FAKE_KEY))
    }

    /// `llx args…` signed in with [`FAKE_KEY`], thinking with the scripted `model`.
    pub(crate) fn with_model(&self, model: &Model, args: &[&str]) -> Command {
        let mut command = self.llx(args);
        command
            .env("OPENROUTER_API_KEY", FAKE_KEY)
            .env("OPENROUTER_BASE_URL", &model.base);
        command
    }

    /// `llx args…` thinking with `model`, `stdin` piped in, run to its end as CI runs it: in
    /// this home, with no terminal at all (a session of its own, so not even `/dev/tty` is there
    /// to ask on).
    pub(crate) fn exec(&self, model: &Model, args: &[&str], stdin: &str) -> Output {
        self.exec_command(self.with_model(model, args), stdin)
    }

    /// `command` run as [`Home::exec`] runs it.
    pub(crate) fn exec_command(&self, mut command: Command, stdin: &str) -> Output {
        detach(&mut command);
        let mut child = command
            .current_dir(&self.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        feed(&mut child, stdin);
        child.wait_with_output().unwrap()
    }

    /// A saved session, as the store writes it, last updated at `updated`: one turn, "fix the
    /// parser".
    pub(crate) fn save_session(&self, id: &str, workspace: &str, updated: u64) {
        let turns = json!([{"goal": "fix the parser", "answer": "Fixed.", "status": "done",
            "at": 1700000000}]);
        self.save_turns(id, workspace, updated, &turns);
    }

    /// A saved session, as the store writes it, with the tasks `turns` (their JSON array).
    pub(crate) fn save_turns(&self, id: &str, workspace: &str, updated: u64, turns: &Value) {
        let dir = self.0.join("sessions");
        std::fs::create_dir_all(&dir).unwrap();
        let session = json!({"id": id, "created": 1700000000, "updated": updated,
            "workspace": workspace, "turns": turns});
        std::fs::write(dir.join(format!("{id}.json")), session.to_string()).unwrap();
    }

    /// The saved session `id`, as JSON.
    pub(crate) fn session(&self, id: &str) -> Value {
        let file = self.0.join("sessions").join(format!("{id}.json"));
        serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        stop_daemon(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `command` run to its end: what it wrote, and how it ended.
#[expect(
    clippy::disallowed_methods,
    reason = "a test runs the binary, or a fixture such as `env`, to its end, against a scripted model"
)]
pub(crate) fn output(command: &mut Command) -> Output {
    command.output().unwrap()
}

/// `stdin` written to `child`'s piped stdin, which is then closed. A run may end before it reads
/// all of it (a usage error, a read that stops at its limit): what it left unread is not the
/// test's failure, how it ended is.
pub(crate) fn feed(child: &mut Child, stdin: &str) {
    let mut input = child.stdin.take().unwrap();
    if let Err(error) = input.write_all(stdin.as_bytes()) {
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe, "{error}");
    }
}

/// `command`'s child leads a session of its own, with no terminal.
pub(crate) fn detach(command: &mut Command) {
    let lead = || {
        // SAFETY: setsid takes nothing and only makes the child a session's leader.
        unsafe { libc::setsid() };
        Ok(())
    };
    // SAFETY: between fork and exec the closure only calls setsid, which is async-signal-safe and
    // touches only the child.
    unsafe { command.pre_exec(lead) };
}

/// `command`'s child leads a session of its own whose terminal is its stdin (a pseudo-terminal),
/// so `/dev/tty` is that one and never the terminal the tests run in.
pub(crate) fn own_terminal(command: &mut Command) {
    let lead = || {
        // SAFETY: setsid takes nothing and only makes the child a session's leader.
        unsafe { libc::setsid() };
        // SAFETY: TIOCSCTTY takes no argument; it only sets the session's terminal.
        unsafe { libc::ioctl(0, libc::TIOCSCTTY as _, 0) };
        Ok(())
    };
    // SAFETY: between fork and exec the closure only calls setsid and ioctl, which are
    // async-signal-safe and touch only the child.
    unsafe { command.pre_exec(lead) };
}

/// The daemon a test started in `home`, if any is left: asked to stop as a person asks
/// (`llx serve stop`, so an instrumented binary writes its coverage), then killed by the pid its
/// lock file still names on its first line (the second is its socket). A daemon that ended
/// cleanly emptied that file.
#[expect(
    clippy::disallowed_methods,
    reason = "`llx serve stop` waits a bounded time for the daemon, and ends"
)]
pub(crate) fn stop_daemon(home: &Path) {
    let lock = home.join("config").join("llx").join("daemon.lock");
    let pid = || {
        let text = std::fs::read_to_string(&lock).ok()?;
        text.lines().next()?.trim().parse::<i32>().ok()
    };
    if pid().is_none() {
        return;
    }
    let mut command = Command::new(env!("CARGO_BIN_EXE_llx"));
    isolate(&mut command, home);
    let _ = (command.args(["serve", "stop"]))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if let Some(pid) = pid() {
        // SAFETY: kill sends a signal and touches nothing of this process.
        unsafe { libc::kill(pid, libc::SIGKILL) };
    }
}

/// The environment every run of the binary gets, and nothing else of the shell that runs the
/// tests: no provider key, no setting, and a model address that fails at once, so a test never
/// depends on the machine and never reaches a real model.
pub(crate) fn isolate(command: &mut Command, home: &Path) {
    command.env_clear();
    // What the system itself needs to start a process and find a temporary directory, and where
    // an instrumented binary writes its coverage.
    for name in ["PATH", "TMPDIR", "LLVM_PROFILE_FILE"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("LLX_SESSIONS", home.join("sessions"))
        .env("LLX_MODELS_REFRESH", "0")
        .env("OPENROUTER_BASE_URL", "http://127.0.0.1:1");
}

/// Bytes a process wrote, as text.
pub(crate) fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
