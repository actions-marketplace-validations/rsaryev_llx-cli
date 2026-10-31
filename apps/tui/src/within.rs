//! A child process the interface waits for, with a deadline (R10): its output read on a thread
//! of its own (or its input written on one, and its exit waited for), and past the deadline it
//! is killed.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// What `command` printed, given nothing to read, if it ended well within `wait`; past it, it is
/// killed. What is waited for is the output's end.
pub(super) fn within(mut command: Command, wait: Duration) -> Option<Vec<u8>> {
    let mut child = (command.stdin(Stdio::null()))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let printed = child.stdout.take().and_then(|out| {
        let (read, done) = mpsc::sync_channel(1);
        let reader = thread::Builder::new().name("llx-child-out".into());
        let reading = reader.spawn(move || {
            let mut bytes = vec![];
            let _ = out.take(64 << 20).read_to_end(&mut bytes);
            let _ = read.send(bytes);
        });
        reading.ok().and_then(|_| done.recv_timeout(wait).ok())
    });
    let Some(printed) = printed else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    // Its output ended as it exited: the status is there to be read.
    let ended = child.wait().ok()?;
    ended.success().then_some(printed)
}

/// Whether `command` took `input` and exited well within `wait`; past it, it is killed. Its
/// output goes nowhere and is not waited for: a clipboard tool may leave a child behind that
/// keeps it open (xclip's, serving the selection), so only the exit says it is done.
pub(super) fn exits_within(mut command: Command, input: Vec<u8>, wait: Duration) -> bool {
    let spawned = (command.stdin(Stdio::piped()))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = spawned else {
        return false;
    };
    give(&mut child, input);
    let deadline = Instant::now() + wait;
    let mut pause = Duration::from_millis(1);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => {}
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
        // std gives no event for a child's exit: looked at again, a little later each time.
        let left = deadline.saturating_duration_since(Instant::now());
        #[expect(
            clippy::disallowed_methods,
            reason = "a poll's backoff, bounded by the deadline: std has no wait with a timeout"
        )]
        thread::sleep(pause.min(left));
        pause = (pause * 2).min(Duration::from_millis(50));
    }
}

/// `input` written to the child on a thread of its own: a child that never reads must not hold
/// the caller.
fn give(child: &mut Child, input: Vec<u8>) {
    if let Some(mut stdin) = child.stdin.take() {
        let writer = thread::Builder::new().name("llx-child-in".into());
        let _ = writer.spawn(move || {
            let _ = stdin.write_all(&input);
        });
    }
}

#[cfg(test)]
#[path = "within_tests.rs"]
mod tests;
