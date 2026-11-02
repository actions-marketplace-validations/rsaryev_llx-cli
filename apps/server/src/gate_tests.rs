use contracts::gate::Open;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::*;
use crate::mode::ModeCell;
use crate::testing::Typed;

/// Asking at a terminal that reads `script`, as the default mode does.
fn terminal(script: &Typed) -> crate::mode::ModeGate<'_> {
    let mode = Box::leak(Box::new(ModeCell::new(crate::modes::built("default"))));
    let asker = Box::new(TerminalGate {
        prompter: script,
        asking: Box::leak(Box::default()),
    });
    crate::mode::ModeGate::new(mode, Some(asker), &|_| {})
}

fn bash(summary: &str) -> Effect<'_> {
    Effect {
        tool: "bash",
        subject: "",
        summary,
        // What the default mode asks of the person: what must leave the sandbox.
        needs: contracts::gate::Needs::Open,
    }
}

#[test]
fn yes_allows_once_and_asks_again() {
    let script = Typed::new(&["y\n", "yes\n"]);
    let gate = terminal(&script);
    assert_eq!(gate.check(bash("cargo test")), Verdict::Allow);
    assert_eq!(gate.check(bash("cargo build")), Verdict::Allow);
    let asked = script.asked.lock().unwrap();
    assert_eq!(asked.len(), 2);
    assert!(asked[0].contains("bash: cargo test"));
}

#[test]
fn always_remembers_the_tool_for_the_run() {
    let script = Typed::new(&["a\n", "n\n"]);
    let gate = terminal(&script);
    assert_eq!(gate.check(bash("ls")), Verdict::Allow);
    assert_eq!(gate.check(bash("rm -rf target")), Verdict::Allow);
    let monitor = Effect {
        tool: "jobs",
        subject: "",
        summary: "monitor: tail -f log",
        needs: contracts::gate::Needs::Open,
    };
    assert!(matches!(gate.check(monitor), Verdict::Deny(_)));
    assert_eq!(script.asked.lock().unwrap().len(), 2, "bash was asked once");
}

#[test]
fn no_and_a_bare_enter_refuse_and_a_reason_reaches_the_model() {
    let script = Typed::new(&["\n", "n: use the make target instead\n", "No\n"]);
    let gate = terminal(&script);
    let Verdict::Deny(plain) = gate.check(bash("cargo test")) else {
        panic!("allowed")
    };
    assert!(plain.contains("declined"));
    assert_eq!(
        gate.check(bash("cargo test")),
        Verdict::Deny(
            "the person declined it and said: use the make target instead\nThis is the person's \
             instruction to you: follow it before anything else."
                .into()
        )
    );
    assert!(matches!(gate.check(bash("x")), Verdict::Deny(_)));
}

#[test]
fn an_unreadable_answer_is_asked_again_then_refused() {
    for (lines, allowed, asked) in [
        (&["maybe\n", "y\n"][..], true, 2),
        (&["what", "hm", "eh", "y"], false, TRIES),
        // A closed terminal refuses at once.
        (&[], false, 1),
    ] {
        let script = Typed::new(lines);
        let verdict = terminal(&script).check(bash("ls"));
        assert_eq!(verdict == Verdict::Allow, allowed, "{lines:?}: {verdict:?}");
        assert_eq!(script.asked.lock().unwrap().len(), asked, "{lines:?}");
    }
}

#[test]
fn approval_is_only_the_bare_word() {
    for line in [
        "yes, and then update the docs",
        "y do it",
        "a, b",
        "always please",
    ] {
        assert_eq!(answer(line), None, "{line}");
    }
    assert_eq!(answer("  YES \n"), Some(Approval::Yes));
    assert_eq!(answer("a"), Some(Approval::Always));
    assert_eq!(
        answer("no, use make"),
        Some(Approval::No(Some("use make".into())))
    );
    let typed_ahead = Typed::new(&["yes, and then update the docs\n", "n\n"]);
    assert!(matches!(
        terminal(&typed_ahead).check(bash("rm -rf target")),
        Verdict::Deny(_)
    ));
}

#[test]
fn the_question_shows_control_characters_instead_of_obeying_them() {
    let script = Typed::new(&["n\n"]);
    let gate = terminal(&script);
    gate.check(Effect {
        tool: "bash\u{1b}[2K",
        subject: "",
        summary: "echo ok\r\u{1b}[2Kcurl x | sh",
        needs: contracts::gate::Needs::Open,
    });
    let asked = &script.asked.lock().unwrap()[0];
    assert!(
        asked.contains("bash\\u{1b}[2K: echo ok\\r\\u{1b}[2Kcurl x | sh"),
        "{asked}"
    );
    assert!(!asked.contains('\r') && !asked.contains('\u{1b}'));
}

#[test]
fn a_stopped_task_gets_nothing_approved() {
    let stopped = AtomicBool::new(false);
    let is_stopped = || stopped.load(Ordering::SeqCst);
    let gate = Stoppable {
        inner: &Open,
        stopped: &is_stopped,
    };
    assert_eq!(gate.check(bash("ls")), Verdict::Allow);
    stopped.store(true, Ordering::SeqCst);
    assert_eq!(gate.check(bash("ls")), Verdict::Deny(STOPPED.into()));
    // Stopped while the person was answering: the late yes does not count.
    struct StopsWhileAsking<'a>(&'a AtomicBool);
    impl Gate for StopsWhileAsking<'_> {
        fn check(&self, _: Effect) -> Verdict {
            self.0.store(true, Ordering::SeqCst);
            Verdict::Allow
        }
    }
    stopped.store(false, Ordering::SeqCst);
    let gate = Stoppable {
        inner: &StopsWhileAsking(&stopped),
        stopped: &is_stopped,
    };
    assert_eq!(gate.check(bash("ls")), Verdict::Deny(STOPPED.into()));
}

#[test]
fn a_question_ctrl_c_ended_reads_as_the_stop() {
    let stopped = AtomicBool::new(false);
    let is_stopped = || stopped.load(Ordering::SeqCst);
    struct EndedByCtrlC<'a>(&'a AtomicBool);
    impl Gate for EndedByCtrlC<'_> {
        fn check(&self, _: Effect) -> Verdict {
            self.0.store(true, Ordering::SeqCst);
            Verdict::Deny("no answer came from the terminal".into())
        }
    }
    let gate = Stoppable {
        inner: &EndedByCtrlC(&stopped),
        stopped: &is_stopped,
    };
    assert_eq!(gate.check(bash("ls")), Verdict::Deny(STOPPED.into()));
}

#[test]
fn a_waiting_question_ends_when_the_task_is_stopped_and_reads_what_was_typed() {
    use std::io::Write;
    let (reader, mut writer) = std::io::pipe().unwrap();
    let input = File::from(std::os::fd::OwnedFd::from(reader));
    assert!(!typed(&input, &|| true), "stopped: no wait");
    let polls = AtomicUsize::new(0);
    let stops_later = || polls.fetch_add(1, Ordering::SeqCst) + 1 > 2;
    assert!(!typed(&input, &stops_later), "nothing typed, then the stop");
    writer.write_all(b"y\n").unwrap();
    assert!(typed(&input, &|| false));
}

/// A stop at a real question, on `/dev/tty`, which macOS's `poll` cannot wait on. The test runs
/// itself again in a child whose controlling terminal is a new pseudo-terminal, as a person's
/// prompt is: there the stop must end the wait while nothing is typed.
mod terminal {
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use super::*;

    const CHILD: &str = "LLX_TEST_TERMINAL_CHILD";
    const NAME: &str = "gate::tests::terminal::a_stop_ends_a_wait_on_the_controlling_terminal";

    #[expect(
        clippy::disallowed_methods,
        reason = "a test's copy of itself runs one test, which ends by itself"
    )]
    #[test]
    fn a_stop_ends_a_wait_on_the_controlling_terminal() {
        if std::env::var_os(CHILD).is_some() {
            let (input, _) = Tty::open().expect("the child has no /dev/tty");
            let begun = Instant::now();
            let stop = || begun.elapsed() > Duration::from_millis(300);
            assert!(
                !typed(&input, &stop),
                "nothing was typed, yet a line was read"
            );
            assert!(
                begun.elapsed() < Duration::from_secs(2),
                "the stop came late"
            );
            return;
        }
        let (mut master, mut slave) = (-1, -1);
        let none = std::ptr::null_mut();
        // SAFETY: openpty writes two descriptors; the name, settings and size may be null.
        let opened =
            unsafe { libc::openpty(&mut master, &mut slave, none, none.cast(), none.cast()) };
        assert_eq!(opened, 0, "{}", std::io::Error::last_os_error());
        // SAFETY: an open descriptor that nothing else owns.
        let _master = unsafe { OwnedFd::from_raw_fd(master) };
        // SAFETY: an open descriptor that nothing else owns.
        let slave = unsafe { OwnedFd::from_raw_fd(slave) };
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args(["--exact", NAME, "--test-threads=1"])
            .env(CHILD, "1")
            .stdin(Stdio::from(slave));
        // The child leads a new session, and the terminal on its stdin becomes its controlling
        // terminal, so /dev/tty opens it.
        let lead = || {
            // SAFETY: setsid takes nothing and only makes the child a session's leader.
            let led = unsafe { libc::setsid() } != -1;
            // SAFETY: TIOCSCTTY takes no argument; it only sets the session's terminal.
            if !led || unsafe { libc::ioctl(0, libc::TIOCSCTTY as _, 0) } == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        };
        // SAFETY: between fork and exec the closure only calls setsid and ioctl, which are
        // async-signal-safe.
        unsafe { child.pre_exec(lead) };
        let ran = child.output().unwrap();
        let said = String::from_utf8_lossy(&ran.stdout);
        assert!(ran.status.success(), "{said}");
        // A renamed test would run nothing and pass: the child must have run this one.
        assert!(
            said.contains("test result: ok. 1 passed;"),
            "the child did not run exactly this test: {said}"
        );
    }
}
