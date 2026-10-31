use super::*;
use crate::testing::Scratch;

const LONG: Duration = Duration::from_secs(30);

fn never() -> bool {
    false
}

fn job<'a>(command: &'a str, dir: &'a Path, timeout: Duration) -> Job<'a> {
    Job {
        command,
        dir,
        timeout,
        hidden: &[],
        stop: &never,
        scope: Scope::Open,
        workspace: Box::leak(Box::new(Workspace::new(dir).unwrap())),
    }
}

#[test]
fn a_command_returns_its_code_and_both_streams_in_the_order_written() {
    let dir = Scratch::new();
    let command = "echo out; echo err 1>&2; echo out again; echo err again 1>&2; exit 3";
    let finished = run(&job(command, dir.path(), LONG)).unwrap();
    assert_eq!(finished.ending, Ending::Exit(3));
    assert_eq!(finished.output, "out\nerr\nout again\nerr again");
    assert!(!finished.open);
}

#[test]
fn hidden_variables_never_reach_the_command() {
    let dir = Scratch::new();
    // A variable the test process always has, and the shell does not need to start.
    let (name, command) = (
        "HOME",
        "if [ -n \"${HOME+x}\" ]; then echo seen; else echo hidden; fi",
    );
    let shown = run(&job(command, dir.path(), LONG)).unwrap();
    assert_eq!(shown.output, "seen");
    let hidden = run(&Job {
        hidden: &[name.to_string()],
        ..job(command, dir.path(), LONG)
    })
    .unwrap();
    assert_eq!(hidden.output, "hidden");
}

#[test]
fn a_command_has_no_terminal_to_prompt_on() {
    let dir = Scratch::new();
    let finished = run(&job(
        "( : </dev/tty ) 2>/dev/null && echo terminal || echo none; echo \"$GIT_TERMINAL_PROMPT\"",
        dir.path(),
        LONG,
    ))
    .unwrap();
    assert_eq!(finished.output, "none\n0");
}

#[test]
fn a_command_knows_an_agent_reads_it_and_no_terminal_draws_colours() {
    // Test runners print only what failed when they see `AGENT` (bun); colours are asked off, and
    // a forcing of them the person's environment has is dropped.
    let dir = Scratch::new();
    let vars = [
        ("FORCE_COLOR".to_string(), "1".to_string()),
        ("PATH".to_string(), std::env::var("PATH").unwrap()),
    ];
    let command = "echo \"$AGENT $NO_COLOR $TERM ${FORCE_COLOR-unset}\"";
    let finished = run(&Job {
        workspace: &Workspace::new(dir.path()).unwrap().with_env(vars.to_vec()),
        ..job(command, dir.path(), LONG)
    })
    .unwrap();
    assert_eq!(finished.output, "1 1 dumb unset");
}

#[test]
fn a_timeout_kills_the_command_and_what_it_started() {
    let dir = Scratch::new();
    let started = Instant::now();
    // The shell waits on a child, which holds the pipes too: both must die for the call to end.
    let finished = run(&job(
        "sleep 5; echo never",
        dir.path(),
        Duration::from_millis(300),
    ))
    .unwrap();
    assert_eq!(finished.ending, Ending::TimedOut { killed: true });
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    assert!(finished.elapsed >= Duration::from_millis(300));
    assert_eq!(finished.output, "");
    assert!(!finished.open);
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test looks again until it is so, for a bounded time: there is no event to wait on; kill -0 ends by itself"
)]
#[test]
fn a_stop_kills_the_command_and_what_it_started() {
    let dir = Scratch::new();
    let started = Instant::now();
    let stop = || started.elapsed() >= Duration::from_millis(200);
    let finished = run(&Job {
        stop: &stop,
        ..job("sleep 30 & echo $! > child.pid; wait", dir.path(), LONG)
    })
    .unwrap();
    assert_eq!(finished.ending, Ending::Stopped { killed: true });
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    let pid = std::fs::read_to_string(dir.path().join("child.pid")).unwrap();
    let alive = || {
        Command::new("kill")
            .args(["-0", pid.trim()])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    };
    // The orphan is reaped by init shortly after it dies.
    let deadline = Instant::now() + Duration::from_secs(2);
    while alive() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!alive(), "the background sleep outlived the stop");
}

/// A command's end is noticed when its output closes, not at the next look of a poll: let end
/// right after the runner looked at it (as a poll would then sleep its longest), it is told at
/// once.
#[test]
fn a_command_s_end_is_noticed_at_once() {
    let dir = Scratch::new();
    let fifo = dir.path().join("go");
    let made = Command::new("mkfifo").arg(&fifo).spawn().unwrap().wait();
    assert!(made.unwrap().success());
    let started = Instant::now();
    let ended: Mutex<Option<Instant>> = Mutex::new(None);
    let look = || {
        let mut ended = ended.lock().unwrap();
        if ended.is_none() && started.elapsed() > Duration::from_millis(300) {
            // Its reader is waiting on it already: opening it for writing does not wait.
            std::fs::write(&fifo, "x\n").unwrap();
            *ended = Some(Instant::now());
        }
        false
    };
    let finished = run(&Job {
        stop: &look,
        ..job("read x < go", dir.path(), LONG)
    })
    .unwrap();
    let noticed = ended.lock().unwrap().unwrap().elapsed();
    assert_eq!(finished.ending, Ending::Exit(0));
    assert!(
        noticed < Duration::from_millis(25),
        "noticed {noticed:?} after it ended"
    );
}

#[test]
fn a_signal_is_told_apart_from_an_exit() {
    let dir = Scratch::new();
    let finished = run(&job("kill -s TERM $$", dir.path(), LONG)).unwrap();
    assert_eq!(finished.ending, Ending::Signal(15));
}

#[test]
fn a_background_process_holding_the_output_is_not_waited_for() {
    let dir = Scratch::new();
    let started = Instant::now();
    let finished = run(&job("echo up; sleep 20 &", dir.path(), LONG)).unwrap();
    assert_eq!(finished.ending, Ending::Exit(0));
    // Well before the sleep ends, with room for a loaded machine.
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(finished.output, "up");
    assert!(finished.open);
}

#[test]
fn much_output_on_both_streams_never_stalls() {
    let dir = Scratch::new();
    let command =
        "i=0; while [ $i -lt 20000 ]; do echo line $i; echo line $i 1>&2; i=$((i+1)); done";
    let finished = run(&job(command, dir.path(), LONG)).unwrap();
    assert_eq!(finished.ending, Ending::Exit(0));
    assert!(finished.output.contains(" cut ("), "{}", finished.output);
    assert_eq!(finished.cut_lines, Some(40_000));
}

/// A descriptor this process has open without close-on-exec when a command starts never reaches
/// it: the command gets its standard streams, and nothing else.
#[test]
fn a_command_inherits_only_its_standard_streams() {
    let dir = Scratch::new();
    let (_stray, probe) = crate::testing::stray();
    let finished = run(&job(&probe, dir.path(), LONG)).unwrap();
    assert_eq!(finished.output, "closed");
}

/// Commands started at once from several threads, as a turn's calls are, some leaving a process in
/// the background with its own output closed. On macOS std makes a pipe, then marks it
/// close-on-exec a moment later, so a command another thread forks in that moment gets it, and
/// its background process keeps it: the output of the command whose pipe it was stays open (told
/// as held by a background process it never started), and the pipe a start reads to learn whether
/// exec succeeded keeps that start waiting until the background process ends. No command gets a
/// descriptor it was not given.
#[test]
fn commands_started_at_once_never_hold_each_other_s_pipes() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    // A quarter of the threads leave a process behind each time: enough forks landing in another
    // thread's moment to fail every run without the fix, and processes far below the system's
    // limit (240, for 3 s).
    const THREADS: usize = 24;
    const EACH: usize = 40;
    let dir = Scratch::new();
    let (slow, held) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let start = |command: &str| {
        for _ in 0..EACH {
            let started = Instant::now();
            let finished = run(&job(command, dir.path(), LONG)).unwrap();
            assert_eq!(finished.ending, Ending::Exit(0));
            // A start that waited on a background process waits as long as it lives.
            if started.elapsed() > Duration::from_secs(2) {
                slow.fetch_add(1, Ordering::Relaxed);
            }
            if finished.open {
                held.fetch_add(1, Ordering::Relaxed);
            }
        }
    };
    let start = &start;
    thread::scope(|scope| {
        for each in 0..THREADS {
            let command = match each % 4 {
                0 => "sleep 3 </dev/null >/dev/null 2>&1 &",
                _ => ":",
            };
            let named = thread::Builder::new().name("llx-test-start".into());
            named.spawn_scoped(scope, move || start(command)).unwrap();
        }
    });
    let (slow, held) = (slow.into_inner(), held.into_inner());
    assert_eq!(
        (slow, held),
        (0, 0),
        "of {} commands, {slow} took over 2 s to start and {held} had their output held open",
        THREADS * EACH
    );
}
