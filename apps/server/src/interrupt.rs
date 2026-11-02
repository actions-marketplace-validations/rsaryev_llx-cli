//! The person stopping a task: Ctrl-C (SIGINT), SIGTERM or SIGHUP (the terminal gone) during it.
//! The task ends the way the time limit ends it: the command running is killed with everything it
//! started (commands run in their own session, so the terminal's signal never reaches them); the
//! gate approves nothing more, and the task ends at its next model turn as `interrupted` (exit
//! 130), with its `run.completed` event and its session saved. A second signal, or one while no
//! task runs (at the prompt line), quits at once. A signal stops every session's task; the
//! interface's keys stop one session's, by its id.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::sync::lock;

/// What the signal handler stops: every task running, in every session.
/// Only atomics, so a signal handler may use it.
pub(crate) struct Process {
    /// Raised by each stop: a task that began before it is stopped.
    generation: AtomicU64,
    /// How many tasks run.
    busy: AtomicUsize,
    /// Stopped while tasks ran: the next stop quits, until none runs.
    pending: AtomicBool,
}

/// The process's own: what its signal handler stops.
pub(crate) static PROCESS: Process = Process::new();

impl Process {
    const fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
            busy: AtomicUsize::new(0),
            pending: AtomicBool::new(false),
        }
    }

    /// Stop every task running. Returns whether to quit instead: none runs, or they were
    /// stopped already.
    pub(crate) fn stop(&self) -> bool {
        if self.busy.load(Ordering::SeqCst) == 0 {
            return true;
        }
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.pending.swap(true, Ordering::SeqCst)
    }

    /// Whether the tasks running were stopped.
    pub(crate) fn stopping(&self) -> bool {
        self.pending.load(Ordering::SeqCst)
    }
}

/// One session's task: whether it runs, and whether it was stopped, by itself or by the process.
pub(crate) struct Interrupt {
    process: &'static Process,
    busy: AtomicBool,
    stopped: AtomicBool,
    /// The process's generation when the task began.
    began: AtomicU64,
}

impl Interrupt {
    /// A session's, stopped by the process's signals too.
    pub(crate) fn new() -> Self {
        Self::of(&PROCESS)
    }

    fn of(process: &'static Process) -> Self {
        Self {
            process,
            busy: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            began: AtomicU64::new(process.generation.load(Ordering::SeqCst)),
        }
    }

    /// A task starts: nothing has stopped it yet, not even a stop of the process before it.
    pub(crate) fn begin(&self) {
        self.stopped.store(false, Ordering::SeqCst);
        let now = self.process.generation.load(Ordering::SeqCst);
        self.began.store(now, Ordering::SeqCst);
        if !self.busy.swap(true, Ordering::SeqCst) {
            self.process.busy.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// The task ended.
    pub(crate) fn end(&self) {
        if self.busy.swap(false, Ordering::SeqCst)
            && self.process.busy.fetch_sub(1, Ordering::SeqCst) == 1
        {
            self.process.pending.store(false, Ordering::SeqCst);
        }
    }

    /// Whether the task was stopped: by the person, or by a stop of the process since it began.
    pub(crate) fn stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
            || self.began.load(Ordering::SeqCst) != self.process.generation.load(Ordering::SeqCst)
    }

    /// Stop the task. Returns whether to quit instead: no task runs, or it was stopped already.
    pub(crate) fn stop(&self) -> bool {
        !self.busy.load(Ordering::SeqCst) || self.stopped.swap(true, Ordering::SeqCst)
    }
}

type Stop = Arc<dyn Fn() -> bool + Send + Sync>;

/// The sessions', agents' and jobs' stops by id, for the person's keys in the interface, each
/// with the number its [`Known`] removes it by. Never used by the signal handler.
static STOPS: Mutex<Vec<(String, u64, Stop)>> = Mutex::new(Vec::new());
static NEXT: AtomicU64 = AtomicU64::new(0);

/// A stop known by its id until this is dropped, when what it stops has ended: a stale id then
/// stops nothing.
#[must_use = "the stop is forgotten when this is dropped"]
pub(crate) struct Known(u64);

impl Drop for Known {
    fn drop(&mut self) {
        lock(&STOPS).retain(|(_, number, _)| *number != self.0);
    }
}

/// Know `stop` as the stop of session `id`: it returns whether to quit instead.
pub(crate) fn register(id: &str, stop: impl Fn() -> bool + Send + Sync + 'static) -> Known {
    let number = NEXT.fetch_add(1, Ordering::SeqCst);
    lock(&STOPS).push((id.to_string(), number, Arc::new(stop)));
    Known(number)
}

/// Know `flag` as the stop of agent or job `id`: a stop never quits for it.
pub(crate) fn register_flag(id: &str, flag: Arc<AtomicBool>) -> Known {
    register(id, move || {
        flag.store(true, Ordering::SeqCst);
        false
    })
}

/// What job `id` of lane `lane` is known by: each lane's jobs are numbered from one.
pub(crate) fn job_key(lane: &str, id: &str) -> String {
    format!("{lane}{JOB}{id}")
}

/// What sets a job's key apart: no session's or agent's id has it.
const JOB: char = '/';

/// The monitors and schedules running, in every lane of the process.
pub(crate) fn jobs() -> usize {
    let stops = lock(&STOPS);
    stops.iter().filter(|(id, _, _)| id.contains(JOB)).count()
}

/// Stop session `id`'s task: whether to quit instead (it was stopped already); an unknown one
/// is left alone.
pub(crate) fn stop(id: &str) -> bool {
    let found = lock(&STOPS)
        .iter()
        .rev()
        .find(|(known, _, _)| known == id)
        .map(|(_, _, stop)| Arc::clone(stop));
    let quit = found.is_some_and(|stop| stop());
    // A question the task waits on withdraws now, not at its next look.
    crate::server::ui::ring();
    quit
}

/// Tasks running in the process, in every session.
pub(crate) fn working() -> usize {
    PROCESS.busy.load(Ordering::SeqCst)
}

/// The write end of the pipe that wakes the quitting thread, or -1 before [`install`].
static QUIT_FD: AtomicI32 = AtomicI32::new(-1);

/// Quit now, as a second Ctrl-C does: every command and monitor started is killed first (their
/// sessions outlive the process otherwise), by a thread that can do what a signal handler cannot;
/// this only wakes it, and may be called from a handler.
pub fn quit_now() -> ! {
    let fd = QUIT_FD.load(Ordering::SeqCst);
    // SAFETY: write is async-signal-safe and reads one static byte.
    if fd >= 0 && unsafe { libc::write(fd, b"q".as_ptr().cast(), 1) } == 1 {
        loop {
            // SAFETY: pause only waits for a signal; the quitting thread ends the process.
            unsafe { libc::pause() };
        }
    }
    // SAFETY: _exit ends the process at once.
    unsafe { libc::_exit(crate::exit::INTERRUPTED) }
}

/// Start the thread that ends the process when [`quit_now`] wakes it. Without one (no pipe, no
/// thread) a quit is a plain `_exit`.
fn reaper() {
    let mut ends = [-1 as libc::c_int; 2];
    // SAFETY: pipe fills the two descriptors of `ends`, which lives here.
    if unsafe { libc::pipe(ends.as_mut_ptr()) } != 0 {
        return;
    }
    let [read, write] = ends;
    let thread = std::thread::Builder::new().name("llx-quit".into());
    let started = thread.spawn(move || {
        let mut byte = 0_u8;
        // SAFETY: read fills one byte of `byte`, which lives here; the descriptor is this thread's.
        while unsafe { libc::read(read, (&raw mut byte).cast(), 1) } < 0 {}
        tools::shell::kill_all();
        // SAFETY: _exit ends the process at once.
        unsafe { libc::_exit(crate::exit::INTERRUPTED) }
    });
    if started.is_ok() {
        QUIT_FD.store(write, Ordering::SeqCst);
    }
}

/// Handle SIGINT, SIGTERM and SIGHUP (the terminal gone) for the rest of the process.
pub(crate) fn install() {
    reaper();
    extern "C" fn on_signal(_: libc::c_int) {
        const NOTE: &[u8] = b"\nllx: stopping the task (Ctrl-C again quits now)\n";
        if PROCESS.stop() {
            quit_now();
        }
        // SAFETY: write is async-signal-safe, and NOTE outlives the call.
        unsafe { libc::write(2, NOTE.as_ptr().cast(), NOTE.len()) };
    }
    // SAFETY: a sigaction is integers, a mask and a handler's address: all zero is one, with
    // no handler, until it is set below.
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
    // SA_RESTART keeps a model request or a read in progress from failing half-way: the task
    // stops at its next turn instead.
    action.sa_flags = libc::SA_RESTART;
    // SAFETY: sigemptyset writes only the mask, which lives here.
    unsafe { libc::sigemptyset(&mut action.sa_mask) };
    // The action is fully initialized before it is installed, and the handler does only what a
    // signal handler may (atomics, write, pause, _exit).
    for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        // SAFETY: as said above; the action outlives the call, which copies it.
        unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) };
    }
}

#[cfg(test)]
#[path = "interrupt_tests.rs"]
mod tests;
