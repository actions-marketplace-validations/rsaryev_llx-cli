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

    /// The settings name `model` as a server on this machine, called `box`.
    pub(crate) fn with_local(&self, model: &Model) {
        self.with_settings(&serde_json::json!({"local": [{"name": "box", "url": model.base}]}));
    }

    /// The settings name `model` as a host anywhere, called `box`, with a key.
    pub(crate) fn with_host(&self, model: &Model) {
        let host = serde_json::json!({"name": "box", "url": model.base, "key": "sk-box"});
        self.with_settings(&serde_json::json!({"hosts": [host]}));
    }

    pub(crate) fn with_settings(&self, settings: &Value) {
        let config = self.0.join("config/llx");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("settings.json"), settings.to_string()).unwrap();
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
    /// parser", that ended then.
    pub(crate) fn save_session(&self, id: &str, workspace: &str, updated: u64) {
        let turns = json!([{"goal": "fix the parser", "answer": "Fixed.", "status": "done",
            "at": updated}]);
        self.save_turns(id, workspace, &turns);
    }

    /// A saved session, as the store writes it, with the tasks `turns` (their JSON array).
    pub(crate) fn save_turns(&self, id: &str, workspace: &str, turns: &Value) {
        let header = json!({"id": id, "created": 1700000000, "workspace": workspace});
        self.write_session(&header, turns);
    }

    /// A session's file as the store writes it (JSON Lines): `header`'s fields on its first line,
    /// then each of `turns`.
    pub(crate) fn write_session(&self, header: &Value, turns: &Value) {
        let dir = self.0.join("sessions");
        std::fs::create_dir_all(&dir).unwrap();
        let typed = |kind: &str, fields: &Value| {
            let mut line = fields.as_object().unwrap().clone();
            line.insert("type".into(), kind.into());
            Value::Object(line).to_string() + "\n"
        };
        let mut text = typed("session", header);
        for turn in turns.as_array().unwrap() {
            text += &typed("turn", turn);
        }
        let id = header["id"].as_str().unwrap();
        std::fs::write(dir.join(format!("{id}.jsonl")), text).unwrap();
    }

    /// The saved session `id`, as one JSON value: its header's fields, its `title` and `turns`.
    pub(crate) fn session(&self, id: &str) -> Value {
        let file = self.0.join("sessions").join(format!("{id}.jsonl"));
        read_session(&std::fs::read_to_string(file).unwrap())
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A session's file (JSON Lines) as one JSON value: its header's fields, then its `title` (the
/// last made) and its `turns`, in order.
pub(crate) fn read_session(text: &str) -> Value {
    let mut lines = text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap());
    let mut session = lines.next().unwrap();
    let mut turns = vec![];
    for line in lines {
        match line["type"].as_str() {
            Some("turn") => turns.push(line),
            Some("title") => session["title"] = line["title"].clone(),
            other => panic!("a line of an unknown type: {other:?}"),
        }
    }
    session["turns"] = Value::Array(turns);
    session
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
        .env("LLX_LOCAL", "0")
        .env("OPENROUTER_BASE_URL", "http://127.0.0.1:1");
}

/// Bytes a process wrote, as text.
pub(crate) fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
