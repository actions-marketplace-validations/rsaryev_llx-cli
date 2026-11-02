//! One command run to its end, its deadline, or until it is stopped. The shell starts a session
//! of its own: it has no controlling terminal, so a program that opens the terminal to
//! prompt (a password, a passphrase) fails at once instead of stopping in the background until the
//! deadline, and the session's processes, found by its id, are everything it started (see
//! `sweep`). Its stdout and stderr are one pipe, as a terminal shows them: in the order the
//! command wrote them, read on a thread of its own.

use std::io::{self, Read};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::output::{Output, Span};
use super::sandbox::{Held, Scope};
use crate::workspace::Workspace;

/// How long the output may stay open after the shell exits: a background process it started may
/// hold the pipes, and is not waited for.
pub(super) const DRAIN_GRACE: Duration = Duration::from_millis(500);
/// How long a killed process is given to be reaped.
const KILL_WAIT: Duration = Duration::from_secs(2);
/// The longest wait between looks at the command: a stop, and an exit that leaves the output open
/// to a background process, are noticed within it.
const POLL: Duration = Duration::from_millis(50);

/// The shell commands run with, as a person would type it.
pub const SHELL: &str = "sh -c";

/// What to run, and the bounds it runs in.
pub(super) struct Job<'a> {
    pub command: &'a str,
    pub dir: &'a Path,
    pub timeout: Duration,
    /// Variables left out of its environment (see [`scoped`]).
    pub hidden: &'a [String],
    /// Whether the person stopped the task: the command is killed as soon as it says so.
    pub stop: &'a (dyn Fn() -> bool + Sync),
    /// What the command may do: anything, change the workspace alone, or read alone (the last
    /// two in the sandbox).
    pub scope: Scope,
    /// Where it may write in the workspace's scope, and what a confined command may not even
    /// read: the app's own files ([`Workspace::denied`]).
    pub workspace: &'a Workspace,
}

/// How a command ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ending {
    Exit(i32),
    /// Ended by a signal it did not handle.
    Signal(i32),
    /// Past its deadline; `killed` once it and everything it started are known to be gone.
    TimedOut {
        killed: bool,
    },
    /// Stopped by the person; `killed` as for a timeout.
    Stopped {
        killed: bool,
    },
}

#[derive(Debug)]
pub(super) struct Finished {
    pub ending: Ending,
    pub elapsed: Duration,
    /// stdout and stderr as one stream, as the model reads it (`output::Output`).
    pub output: String,
    /// Lines printed in all, when something of them was left out.
    pub cut_lines: Option<usize>,
    /// The output was still open when it was taken: a background process holds it.
    pub open: bool,
}

/// Run `job` in a shell, with no input.
pub(super) fn run(job: &Job) -> io::Result<Finished> {
    // Held to the end: the scratch directory goes when it drops.
    let (mut shell, _held) = scoped(job.command, job.scope, job.workspace, job.hidden, Quiet::On)?;
    let (reader, writer) = io::pipe()?;
    let mut child = shell
        .current_dir(job.dir)
        .stdin(Stdio::null())
        .stdout(writer.try_clone()?)
        .stderr(writer)
        .spawn()?;
    // The command holds the pipe's writing ends now: ours go, so its end is the output's end.
    drop(shell);
    // Listed until this run ends (after the reap and the drain), for a quit that cannot wait for it
    // (`leaders`).
    let _listed = super::leaders::track(&child);
    let started = Instant::now();
    let output = Arc::new(Mutex::new(Output::new(std::env::temp_dir())));
    let shared = Arc::clone(&output);
    let reader_ended = pump(reader, "llx-pipe", move |bytes| {
        crate::lock(&shared).push(bytes)
    });
    let bounds: (Instant, &dyn Fn() -> bool) = (started + job.timeout, job.stop);
    let (ending, elapsed, closed) = watch(&mut child, bounds, started, &reader_ended)?;
    let open = !closed && reader_ended.recv_timeout(DRAIN_GRACE).is_err();
    let span = if ending == Ending::Exit(0) {
        Span::Wide
    } else {
        Span::Narrow
    };
    let captured = crate::lock(&output).finish(span);
    Ok(Finished {
        ending,
        elapsed,
        output: captured.text,
        cut_lines: captured.cut.then_some(captured.lines),
        open,
    })
}

/// Whether a command's environment is made quiet for an agent's reading ([`quiet`]), as the
/// `bash` tool's commands are, or kept as the place has it (a monitor's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quiet {
    On,
    Off,
}

/// The environment the `bash` tool's commands run in: nothing that waits for an answer or a
/// pager, since there is no one to type or page, and nothing printed only for a person's eyes:
/// `AGENT` says an agent reads the output, so the tools that look for it (test runners) print
/// what failed and leave out what passed, and colours are asked off (and any forcing of them
/// dropped) on a terminal that draws nothing.
fn quiet(shell: &mut Command) {
    shell
        .env_remove("FORCE_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env("AGENT", "1")
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .env("PAGER", "cat");
}

/// Wait for `child` to exit, killing it at its `deadline` or when `stop` says the person stopped
/// the task; how it ended, when, and whether its output closed meanwhile. A command's exit closes
/// its output, so the wait is on the pipe's reader (`reader_ended`, a message once the pipe
/// closes): the exit is seen at once (R9). The stop is looked at, and an exit that leaves the
/// output to a background process seen, within [`POLL`].
#[expect(
    clippy::disallowed_methods,
    reason = "a backoff of a few ms, bounded by the deadline: the pipes closed and the exit, which \
              std gives no event for, is a moment away"
)]
pub(super) fn watch(
    child: &mut Child,
    (deadline, stop): (Instant, &dyn Fn() -> bool),
    started: Instant,
    reader_ended: &Receiver<()>,
) -> io::Result<(Ending, Duration, bool)> {
    let (mut closed, mut pause) = (false, Duration::from_millis(1));
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok((ending(status), started.elapsed(), closed)),
            Ok(None) => {}
            Err(error) => {
                kill(child);
                return Err(error);
            }
        }
        let late = Instant::now() >= deadline;
        if late || stop() {
            let elapsed = started.elapsed();
            let killed = kill(child);
            let ending = match late {
                true => Ending::TimedOut { killed },
                false => Ending::Stopped { killed },
            };
            return Ok((ending, elapsed, closed));
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if !closed {
            match reader_ended.recv_timeout(POLL.min(left)) {
                // A reader gone without a word: the pipe will say no more either.
                Ok(()) | Err(RecvTimeoutError::Disconnected) => closed = true,
                Err(RecvTimeoutError::Timeout) => {}
            }
        } else {
            thread::sleep(pause.min(left));
            pause = (pause * 2).min(POLL);
        }
    }
}

/// The shell for `command` in `scope`, its environment ready: the variables of the place it runs
/// for (`workspace`'s, the client's), else this process's own, without `hidden` (the app's own
/// secrets, which a command can still read from its parent's environment: see
/// `BashTool::hidden`), made [`quiet`] where `quieted` says so, and a confined command's `TMPDIR`
/// its scratch directory, set last so the place's own does not replace it (the sandbox lets it
/// write there alone, so nothing set after may clear the environment); and what it holds until
/// it ends: an open command holds nothing.
pub fn scoped(
    command: &str,
    scope: Scope,
    workspace: &Workspace,
    hidden: &[String],
    quieted: Quiet,
) -> io::Result<(Command, Held)> {
    let kept = workspace.denied();
    let (mut shell, held) = match scope {
        Scope::Open => (shell(command), Held::default()),
        Scope::Workspace => {
            let writes = super::sandbox::writable(workspace.root());
            super::sandbox::confined(command, writes, kept)?
        }
        Scope::ReadOnly => super::sandbox::confined(command, None, kept)?,
    };
    if let Some(vars) = workspace.env() {
        shell
            .env_clear()
            .envs(vars.iter().map(|(name, value)| (name, value)));
    }
    for name in hidden {
        shell.env_remove(name);
    }
    match quieted {
        Quiet::On => quiet(&mut shell),
        Quiet::Off => {}
    }
    if let Some(scratch) = held.scratch() {
        shell.env("TMPDIR", scratch);
    }
    Ok((shell, held))
}

/// `sh -c command` as the leader of a session of its own (so `kill` ends all it starts), inheriting
/// llx's standard streams only.
pub fn shell(command: &str) -> Command {
    let mut shell = Command::new("sh");
    shell.arg("-c").arg(command);
    lead(&mut shell);
    shell
}

/// Make `shell` the leader of a session of its own when it starts, inheriting llx's standard
/// streams only. A sandbox's own hooks, added after, run after these.
pub(super) fn lead(shell: &mut Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: setsid takes nothing and only makes the forked child a session's leader.
    let lead = || match unsafe { libc::setsid() } {
        -1 => Err(io::Error::last_os_error()),
        _ => Ok(()),
    };
    // SAFETY: setsid is async-signal-safe, and nothing else runs between fork and exec. The new
    // session's process group has the shell's pid, which `kill` signals.
    unsafe { shell.pre_exec(lead) };
    super::inherit::streams_only(shell);
}

/// Kill the command and everything it started: its process group, then every other process of its
/// session (the jobs a shell with job control put in groups of their own); whether all of it is
/// known to be gone. `child` must not be reaped yet: its pid is the session's id until it is.
pub fn kill(child: &mut Child) -> bool {
    // A pid the system gave: it is a pid_t, which `id` widened.
    let leader = child.id().cast_signed();
    // Before the reap, while the leader's pid still names the session; the reap runs either way.
    let ended = super::sweep::end_session(leader, KILL_WAIT);
    reaped(child) && ended
}

/// Whether `child` exited within [`KILL_WAIT`] of being killed.
#[expect(
    clippy::disallowed_methods,
    reason = "a child's exit has no event in std: it is looked at every 10 ms, for at most KILL_WAIT"
)]
fn reaped(child: &mut Child) -> bool {
    let deadline = Instant::now() + KILL_WAIT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            _ => return false,
        }
    }
}

/// Read `pipe` to its end on a thread named `name`, giving `take` each piece as it comes; the
/// receiver hears once it ends. With no thread to read it, the pipe is dropped with the closure,
/// the command sees it closed, and the receiver is disconnected: its output is told as empty.
pub(super) fn pump(
    mut pipe: io::PipeReader,
    name: &str,
    mut take: impl FnMut(&[u8]) + Send + 'static,
) -> Receiver<()> {
    // One reader, one send: room for it, so it never waits.
    let (closed, ended) = mpsc::sync_channel(1);
    let started = thread::Builder::new().name(name.into()).spawn(move || {
        let mut buffer = [0; 65536];
        loop {
            match pipe.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => take(buffer.get(..read).unwrap_or_default()),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        let _ = closed.send(());
    });
    drop(started);
    ended
}

/// How `status` ended. A status `try_wait` gives is an exit or a signal: it waits with neither
/// `WUNTRACED` nor `WCONTINUED`, so the 0 stands in for a state it cannot return.
fn ending(status: ExitStatus) -> Ending {
    status.code().map_or_else(
        || Ending::Signal(status.signal().unwrap_or_default()),
        Ending::Exit,
    )
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
