//! A monitor's wait (B10): its command is a check, run every few seconds, each run in a session
//! of its own (so a stop or its time kills everything it started), with no input, until it
//! exits 0 or its time is up. It ends once, with how and what the last check printed.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::process::Stdio;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use tools::workspace::Workspace;

/// Lines of a check's output kept (its last), and characters of a line.
const LINES: usize = 40;
const LINE_CHARS: usize = 400;
/// How often the stop and the deadline are looked at.
const TICK: Duration = Duration::from_millis(100);
/// How long one check may run, at most: a check is a quick look, not a wait of its own.
const MAX_CHECK: Duration = Duration::from_secs(30);
/// How long a check that exited is given to let its last lines through.
const DRAIN: Duration = Duration::from_millis(200);

/// What a monitor runs, and how.
pub(super) struct Watch<'a> {
    pub(super) command: &'a str,
    pub(super) scope: tools::shell::Scope,
    pub(super) workspace: &'a Workspace,
    pub(super) hidden: &'a [String],
    /// From one check's start to the next's.
    pub(super) every: Duration,
    pub(super) deadline: Instant,
}

/// How a monitor ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ended {
    /// A check exited 0.
    Met,
    TimedOut,
    /// A check that cannot run (126, 127: not executable, not found), or could not start.
    Unrunnable,
    Stopped,
}

/// One check: how it ended (`Exit code: 7`) and the end of what it printed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Check {
    pub(super) how: String,
    pub(super) output: String,
}

/// How the wait ended, the checks run, and the last one.
pub(super) struct Waited {
    pub(super) ended: Ended,
    pub(super) checks: usize,
    pub(super) last: Option<Check>,
}

/// Wait as `watch` says until a check exits 0, its deadline passes or `stopped` says so; give
/// each check to `seen`, with how many ran.
pub(super) fn wait(
    watch: &Watch<'_>,
    stopped: &dyn Fn() -> bool,
    seen: &dyn Fn(usize, &Check),
) -> Waited {
    let mut waited = Waited {
        ended: Ended::TimedOut,
        checks: 0,
        last: None,
    };
    loop {
        if stopped() {
            waited.ended = Ended::Stopped;
            return waited;
        }
        if Instant::now() >= watch.deadline {
            return waited;
        }
        let started = Instant::now();
        let (check, code) = match run(watch, stopped) {
            Ran::Done(check, code) => (check, code),
            Ran::Stopped => {
                waited.ended = Ended::Stopped;
                return waited;
            }
            Ran::Failed(why) => {
                waited.ended = Ended::Unrunnable;
                waited.last = Some(Check {
                    how: format!("Failed: {why}"),
                    output: String::new(),
                });
                return waited;
            }
        };
        waited.checks += 1;
        seen(waited.checks, &check);
        waited.last = Some(check);
        match code {
            Some(0) => {
                waited.ended = Ended::Met;
                return waited;
            }
            Some(126 | 127) => {
                waited.ended = Ended::Unrunnable;
                return waited;
            }
            _ => {}
        }
        // The next check is due at a time, not on an event; the stop is looked at meanwhile.
        let next = (started + watch.every).min(watch.deadline);
        while Instant::now() < next && !stopped() {
            pause(TICK.min(next.saturating_duration_since(Instant::now())));
        }
    }
}

/// Wait `for_` before looking again: at the next check's time, or at a check whose exit std gives
/// no event for.
#[expect(
    clippy::disallowed_methods,
    reason = "a poll's interval, a tick at most, looked at for the stop after each"
)]
fn pause(for_: Duration) {
    std::thread::sleep(for_);
}

/// How one check ran.
enum Ran {
    Done(Check, Option<i32>),
    Stopped,
    Failed(String),
}

/// Read `stream` line by line on a thread of its own into `sent`.
fn pump(stream: impl Read + Send + 'static, sent: mpsc::SyncSender<String>) {
    let reader = std::thread::Builder::new().name("llx-monitor".into());
    let started = reader.spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            if sent.send(line).is_err() {
                return;
            }
        }
    });
    // No thread to read it: the stream is dropped with the closure, and the command sees it
    // closed.
    drop(started);
}

/// Run the check once at the workspace's root, with its environment less the hidden, in the
/// scope the gate allowed (confined: in the sandbox), killed at its own limit or a stop.
fn run(watch: &Watch<'_>, stopped: &dyn Fn() -> bool) -> Ran {
    // The tools' own way to run a command: a session of its own, only the standard streams
    // inherited, and a confined one's scratch `TMPDIR`. Its environment is kept as it is: the
    // quiet variables of `bash` would change what a watched program prints.
    let quiet = tools::shell::Quiet::Off;
    let made = tools::shell::scoped(
        watch.command,
        watch.scope,
        watch.workspace,
        watch.hidden,
        quiet,
    );
    let (mut shell, _held) = match made {
        Ok(made) => made,
        Err(error) => return Ran::Failed(format!("the command could not start: {error}")),
    };
    shell
        .current_dir(watch.workspace.root())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match shell.spawn() {
        Ok(child) => child,
        Err(error) => return Ran::Failed(format!("the command could not start: {error}")),
    };
    // Listed until it ends, for a quit that cannot wait for it.
    let _listed = tools::shell::track(&child);
    let (sent, received) = mpsc::sync_channel(LINES);
    if let Some(out) = child.stdout.take() {
        pump(out, sent.clone());
    }
    if let Some(err) = child.stderr.take() {
        pump(err, sent.clone());
    }
    drop(sent);
    let started = Instant::now();
    let limit = started + watch.every.min(MAX_CHECK);
    let mut kept: VecDeque<String> = VecDeque::new();
    let mut keep = |line: String| {
        if kept.len() == LINES {
            kept.pop_front();
        }
        kept.push_back(line.chars().take(LINE_CHARS).collect());
    };
    let mut exited: Option<(Instant, Option<i32>)> = None;
    let mut closed = false;
    let said = |code: Option<i32>| {
        code.map_or_else(
            || "Killed by a signal".into(),
            |c| format!("Exit code: {c}"),
        )
    };
    let (how, code) = loop {
        if stopped() {
            // Killed only while it runs: once reaped, its pid no longer names its session.
            if exited.is_none() {
                tools::shell::kill(&mut child);
            }
            return Ran::Stopped;
        }
        match received.recv_timeout(TICK) {
            Ok(line) => keep(line),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => closed = true,
        }
        if exited.is_none()
            && let Ok(Some(status)) = child.try_wait()
        {
            exited = Some((Instant::now(), status.code()));
        }
        // Its lines are in once its output closed, or a moment after it exited (what it left in
        // the background may hold the output open, as after a `bash` command).
        if let Some((at, code)) = exited
            && (closed || at.elapsed() >= DRAIN || Instant::now() >= limit)
        {
            break (said(code), code);
        }
        if Instant::now() >= limit {
            tools::shell::kill(&mut child);
            let seconds = started.elapsed().as_secs();
            break (format!("Timed out after: {seconds} seconds (killed)"), None);
        }
        if closed {
            // Its output closed and it has not exited yet: looked at again as the stop is.
            pause(TICK);
        }
    };
    let output = Vec::from(kept).join("\n");
    Ran::Done(Check { how, output }, code)
}

#[cfg(test)]
#[path = "monitor_tests.rs"]
mod tests;
