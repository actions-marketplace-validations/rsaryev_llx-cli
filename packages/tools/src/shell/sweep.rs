//! Every process of a session, killed by its session id. A command's shell leads a session of its
//! own, and one signal to its process group ends only that group: a shell with job control (an
//! interactive shell in a terminal, or `set -m`) puts each job in a group of its own, and a job
//! running in the background gets no hangup when the shell dies. Every process keeps the session
//! it was started in unless it calls `setsid` itself, so the session id finds them all. The
//! session's id is its leader's pid, which stays taken while the leader is not reaped, so the
//! sweep runs before the leader is reaped and never reaches a recycled pid.
//!
//! On macOS the processes are listed into a table on the stack with one system call; on Linux
//! they are the numbered entries of `/proc`.

use std::thread;
use std::time::{Duration, Instant};

/// The most processes one pass looks at: the whole system's, on a table on the stack.
#[cfg(target_os = "macos")]
const MOST: usize = 8192;
/// The pause between passes, while a process that was signalled is still being torn down.
const PAUSE: Duration = Duration::from_millis(10);

/// What one pass found.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct Pass {
    /// Processes of the session, its leader aside, sent SIGKILL.
    pub signalled: usize,
    /// Processes of the session that refused the signal (another user's, such as `sudo`).
    pub refused: usize,
}

/// Send SIGKILL to every process of session `sid` but its leader (the caller kills and reaps that
/// one).
pub(super) fn kill_members(sid: libc::pid_t) -> Pass {
    let mut pass = Pass::default();
    if sid <= 1 {
        return pass;
    }
    each_pid(|pid| {
        // SAFETY: getsid only reads which session a process is in.
        if pid == sid || unsafe { libc::getsid(pid) } != sid {
            return;
        }
        // SAFETY: kill only sends a signal; it touches nothing in this process.
        match unsafe { libc::kill(pid, libc::SIGKILL) } {
            0 => pass.signalled += 1,
            _ if std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) => {}
            _ => pass.refused += 1,
        }
    });
    pass
}

/// Kill every process of session `sid` but its leader, again until none is left (a process may
/// fork while it is killed), for at most `wait`; whether none is left.
#[expect(
    clippy::disallowed_methods,
    reason = "a backoff between kill passes: a process killed has no event to wait on"
)]
pub(super) fn end(sid: libc::pid_t, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    loop {
        let pass = kill_members(sid);
        if pass.refused > 0 {
            return false;
        }
        if pass.signalled == 0 {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(PAUSE);
    }
}

/// Kill the session `leader` leads: its process group (the leader with it: a session's leader
/// cannot leave its group), the leader alone when no process of the group took the signal, then
/// every other process of the session, for at most `wait`; whether all of it is known to be gone.
/// The leader must not be reaped yet: its pid is the session's id until it is.
pub(super) fn end_session(leader: libc::pid_t, wait: Duration) -> bool {
    // SAFETY: kill only sends a signal; it touches nothing in this process.
    let signalled = unsafe { libc::kill(-leader, libc::SIGKILL) } == 0;
    // No process left in the group: everything it started is gone already. Read before the
    // leader's own signal sets errno again.
    let gone = !signalled && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
    if !signalled {
        // SAFETY: as above; the leader itself, when the group's signal missed it.
        unsafe { libc::kill(leader, libc::SIGKILL) };
    }
    let swept = end(leader, wait);
    (signalled || gone) && swept
}

/// Call `visit` with the pid of every process on the system.
#[cfg(target_os = "macos")]
fn each_pid(mut visit: impl FnMut(libc::pid_t)) {
    let mut pids = [0 as libc::pid_t; MOST];
    let size = libc::c_int::try_from(size_of_val(&pids)).unwrap_or(libc::c_int::MAX);
    // SAFETY: proc_listallpids writes at most `size` bytes into `pids`, which outlives the call.
    let count = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), size) };
    let count = usize::try_from(count).unwrap_or(0).min(MOST);
    pids[..count]
        .iter()
        .copied()
        .filter(|&pid| pid > 0)
        .for_each(&mut visit);
}

/// Call `visit` with the pid of every process on the system: the numbered entries of `/proc`.
#[cfg(target_os = "linux")]
fn each_pid(mut visit: impl FnMut(libc::pid_t)) {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return;
    };
    entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
        .for_each(&mut visit);
}

#[cfg(test)]
#[path = "sweep_tests.rs"]
mod tests;
