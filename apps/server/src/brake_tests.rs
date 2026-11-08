use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use contracts::chat::{Message, TaskId};

use super::*;

/// A chat model that answers every request with the same text, and counts them.
#[derive(Default)]
struct Ready(AtomicUsize);

/// A chat model that keeps the deadline each request carried.
#[derive(Default)]
struct Seen(Mutex<Vec<Option<Instant>>>);

impl Chat for Seen {
    #[expect(
        clippy::unwrap_in_result,
        reason = "a test's fake: a poisoned lock fails the test"
    )]
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        self.0.lock().unwrap().push(request.deadline);
        Ok(Completion::default())
    }
}

/// A chat model whose every answer costs 0.01.
struct Dear;

impl Chat for Dear {
    fn complete(&self, _: Request) -> Result<Completion, ChatError> {
        Ok(Completion {
            usage: contracts::chat::Usage {
                cost: 0.01,
                ..Default::default()
            },
            ..Completion::default()
        })
    }
}

impl Chat for Ready {
    fn complete(&self, _: Request) -> Result<Completion, ChatError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(Completion {
            text: Some("ready".into()),
            ..Completion::default()
        })
    }
}

fn request(conversation: &[Message]) -> Request<'_> {
    Request {
        conversation,
        tools: &[],
        calling: contracts::chat::Calling::Any,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    }
}

#[test]
fn past_its_time_a_task_stops_and_the_next_one_starts_fresh() {
    let chat = Ready::default();
    let open = || false;
    let brake = Brake::new(&chat, &open, &open);
    brake.arm(Limits::time(Duration::ZERO));
    assert!(brake.complete(request(&[])).is_err());
    assert_eq!(brake.halted(), Some(Halt::TimedOut));
    assert_eq!(chat.0.load(Ordering::SeqCst), 0, "the model was not called");
    brake.arm(Limits::time(Duration::from_secs(60)));
    assert_eq!(brake.halted(), None);
    assert!(brake.complete(request(&[])).is_ok());
}

/// A chat model whose request is still on its way when the time it was given runs out: the
/// host is cut off at the deadline, as a request held to it is.
struct Slow;

impl Chat for Slow {
    #[expect(
        clippy::disallowed_methods,
        reason = "a test's fake: the request takes this long"
    )]
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        if let Some(at) = request.deadline {
            std::thread::sleep(at.saturating_duration_since(Instant::now()));
        }
        Err(ChatError::Unavailable(
            "ChatGPT did not answer: timeout: global".into(),
        ))
    }
}

#[test]
fn a_request_cut_off_by_the_time_limit_ends_the_task_as_out_of_time_not_as_an_error() {
    let open = || false;
    let brake = Brake::new(&Slow, &open, &open);
    brake.arm(Limits::time(Duration::from_millis(20)));
    let error = brake.complete(request(&[])).unwrap_err();
    assert_eq!(brake.halted(), Some(Halt::TimedOut), "{error}");
    assert_eq!(error.to_string(), "the time limit ran out");
}

#[test]
fn a_wait_past_the_deadline_is_due_and_halts_the_task() {
    let chat = Ready::default();
    let open = || false;
    let brake = Brake::new(&chat, &open, &open);
    assert!(!brake.due(), "no task, no deadline");
    brake.arm(Limits::time(Duration::from_secs(60)));
    assert!(!brake.due());
    assert_eq!(brake.halted(), None);
    brake.arm(Limits::time(Duration::ZERO));
    assert!(brake.due());
    assert_eq!(brake.halted(), Some(Halt::TimedOut));
}

#[test]
fn time_spent_waiting_on_the_person_moves_the_deadline() {
    let chat = Ready::default();
    let open = || false;
    let brake = Brake::new(&chat, &open, &open);
    brake.arm(Limits::time(Duration::ZERO));
    brake.extend(Duration::from_secs(60));
    assert!(
        brake.complete(request(&[])).is_ok(),
        "the wait was given back"
    );
    assert_eq!(brake.halted(), None);
}

#[test]
fn a_closed_output_stops_the_task() {
    let chat = Ready::default();
    let (closed, open) = (|| true, || false);
    let brake = Brake::new(&chat, &closed, &open);
    brake.arm(Limits::time(Duration::from_secs(60)));
    assert!(brake.complete(request(&[])).is_err());
    assert_eq!(brake.halted(), Some(Halt::Closed));
}

#[test]
fn a_stopped_task_halts_at_its_next_turn_and_says_so_after_its_last() {
    let chat = Ready::default();
    let (open, stopped) = (|| false, AtomicBool::new(false));
    let stop = || stopped.load(Ordering::SeqCst);
    let brake = Brake::new(&chat, &open, &stop);
    brake.arm(Limits::time(Duration::from_secs(60)));
    assert!(brake.complete(request(&[])).is_ok());
    stopped.store(true, Ordering::SeqCst);
    assert_eq!(
        brake.halted(),
        Some(Halt::Interrupted),
        "after the last turn"
    );
    assert!(brake.complete(request(&[])).is_err());
    assert_eq!(
        chat.0.load(Ordering::SeqCst),
        1,
        "the model was not asked again"
    );
    assert_eq!(brake.halted(), Some(Halt::Interrupted));
}

#[test]
fn a_task_over_its_budget_halts_before_the_next_request_and_a_new_one_starts_at_zero() {
    let chat = Dear;
    let open = || false;
    let brake = Brake::new(&chat, &open, &open);
    let limits = Limits {
        time: Some(Duration::from_secs(60)),
        budget: Some(0.015),
    };
    brake.arm(limits);
    assert!(brake.complete(request(&[])).is_ok());
    assert!(!brake.due());
    assert!(brake.complete(request(&[])).is_ok(), "0.01 is under 0.015");
    assert!(brake.due(), "0.02 is over it, and a wait ends");
    let refused = brake.complete(request(&[])).unwrap_err();
    assert!(refused.to_string().contains("budget"), "{refused}");
    assert_eq!(brake.halted(), Some(Halt::OverBudget));
    brake.arm(limits);
    assert_eq!(brake.halted(), None);
    assert!(brake.complete(request(&[])).is_ok());
}

#[test]
fn no_budget_never_halts() {
    let chat = Dear;
    let open = || false;
    let brake = Brake::new(&chat, &open, &open);
    brake.arm(Limits::time(Duration::from_secs(60)));
    for _ in 0..50 {
        assert!(brake.complete(request(&[])).is_ok());
    }
    assert!(!brake.due());
}

#[test]
fn a_request_carries_the_time_the_task_has_left() {
    let chat = Seen::default();
    let open = || false;
    let brake = Brake::new(&chat, &open, &open);
    brake.complete(request(&[])).unwrap();
    brake.arm(Limits::time(Duration::from_secs(2)));
    let before = Instant::now();
    brake.complete(request(&[])).unwrap();
    let mut earlier = request(&[]);
    earlier.deadline = Some(before + Duration::from_millis(500));
    brake.complete(earlier).unwrap();
    let seen = chat.0.lock().unwrap().clone();
    assert_eq!(seen[0], None, "no task, no deadline");
    let at = seen[1].unwrap();
    assert!(at > before && at <= Instant::now() + Duration::from_secs(2));
    assert_eq!(
        seen[2],
        Some(before + Duration::from_millis(500)),
        "the nearer wins"
    );
}

#[test]
fn a_task_in_a_session_has_no_time_limit_unless_one_is_given_and_one_in_exec_has_half_an_hour() {
    let in_session = Limits::of(None, None, Attended::Yes);
    assert_eq!(in_session.time, None, "the person stops a task they watch");
    let unattended = Limits::of(None, Some(0.5), Attended::No);
    assert_eq!(unattended.time, Some(Duration::from_secs(1800)));
    assert_eq!(unattended.budget, Some(0.5));
    let given = Limits::of(Some(60), None, Attended::Yes);
    assert_eq!(given.time, Some(Duration::from_secs(60)));
    let (chat, open) = (Ready::default(), || false);
    let brake = Brake::new(&chat, &open, &open);
    brake.arm(in_session);
    assert!(!brake.due());
}
