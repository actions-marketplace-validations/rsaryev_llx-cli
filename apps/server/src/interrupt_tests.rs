use super::*;

#[test]
fn in_a_task_the_first_stop_stops_it_and_the_second_quits() {
    assert!(Interrupt::new().stop(), "outside a task, a stop quits");
    let interrupt = Interrupt::new();
    interrupt.begin();
    assert!(!interrupt.stopped());
    assert!(!interrupt.stop(), "the first stop lets the task end itself");
    assert!(interrupt.stopped());
    assert!(interrupt.stop(), "the second quits");
}

#[test]
fn the_next_task_starts_unstopped() {
    let interrupt = Interrupt::new();
    interrupt.begin();
    interrupt.stop();
    interrupt.end();
    assert!(interrupt.stopped(), "until the next task, it says so");
    interrupt.begin();
    assert!(!interrupt.stopped());
}

/// A process of the test's own: the real one is the signal handler's.
fn process() -> &'static Process {
    Box::leak(Box::new(Process::new()))
}

#[test]
fn stopping_one_lane_leaves_another() {
    let process = process();
    let (one, two) = (Interrupt::of(process), Interrupt::of(process));
    one.begin();
    two.begin();
    assert!(!one.stop());
    assert!(one.stopped());
    assert!(!two.stopped(), "the other session works on");
}

#[test]
fn a_lane_begun_after_a_process_stop_is_not_stopped() {
    let process = process();
    let (one, two) = (Interrupt::of(process), Interrupt::of(process));
    one.begin();
    assert!(!process.stop());
    two.begin();
    assert!(one.stopped());
    assert!(!two.stopped(), "it began after the stop");
}

#[test]
fn process_stop_reaches_every_running_lane() {
    let process = process();
    let (one, two) = (Interrupt::of(process), Interrupt::of(process));
    one.begin();
    two.begin();
    assert!(
        !process.stop(),
        "the first stop lets the tasks end themselves"
    );
    assert!(one.stopped() && two.stopped());
}

#[test]
fn a_second_process_stop_while_busy_quits() {
    let process = process();
    let one = Interrupt::of(process);
    one.begin();
    assert!(!process.stop());
    assert!(process.stop());
}

#[test]
fn process_stop_with_nothing_busy_quits() {
    let process = process();
    assert!(process.stop());
    let one = Interrupt::of(process);
    one.begin();
    one.end();
    assert!(process.stop(), "the task ended: nothing is left to stop");
}

#[test]
fn stop_by_id_returns_true_the_second_time() {
    let one = Arc::new(Interrupt::of(process()));
    one.begin();
    let lane = Arc::clone(&one);
    let _known = register("lane-stopped-twice", move || lane.stop());
    assert!(!stop("lane-stopped-twice"));
    assert!(one.stopped());
    assert!(stop("lane-stopped-twice"), "asked again, it quits");
}

#[test]
fn stop_by_unknown_id_does_nothing() {
    assert!(!stop("no-such-lane"));
}

#[test]
fn a_stop_that_ended_is_not_found() {
    let flag = Arc::new(AtomicBool::new(false));
    let known = register_flag("agent-that-ended", Arc::clone(&flag));
    drop(known);
    assert!(!stop("agent-that-ended"));
    assert!(!flag.load(Ordering::SeqCst), "its entry went with it");
}

#[test]
fn a_job_is_known_by_its_lane() {
    let (one, two) = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    let _one = register_flag(&job_key("lane-one", "job-1"), Arc::clone(&one));
    let _two = register_flag(&job_key("lane-two", "job-1"), Arc::clone(&two));
    stop(&job_key("lane-one", "job-1"));
    assert!(one.load(Ordering::SeqCst));
    assert!(
        !two.load(Ordering::SeqCst),
        "the other lane's job-1 goes on"
    );
}
