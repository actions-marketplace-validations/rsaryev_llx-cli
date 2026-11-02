use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use contracts::chat::{Chat, ChatError, Completion, Message, Request};

use super::*;

/// A chat model that answers `text` and keeps what it was asked, and by when.
struct Namer {
    text: Result<String, ()>,
    asked: Mutex<Vec<(Vec<Message>, usize, bool)>>,
    deadlines: Mutex<Vec<Option<Instant>>>,
}

impl Namer {
    fn new(text: Result<&str, ()>) -> Self {
        Self {
            text: text.map(str::to_string),
            asked: Mutex::new(vec![]),
            deadlines: Mutex::new(vec![]),
        }
    }
}

impl Chat for Namer {
    #[expect(
        clippy::unwrap_in_result,
        reason = "a test's fake: a poisoned lock fails the test"
    )]
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        let asked = (
            request.conversation.to_vec(),
            request.tools.len(),
            request.calling == Calling::Nothing,
        );
        self.asked.lock().unwrap().push(asked);
        self.deadlines.lock().unwrap().push(request.deadline);
        match &self.text {
            Ok(text) => Ok(Completion {
                text: Some(text.clone()),
                ..Completion::default()
            }),
            Err(()) => Err(ChatError::Unavailable("down".into())),
        }
    }
}

#[test]
fn the_model_is_asked_once_with_no_tools_for_the_task_it_names() {
    let namer = Namer::new(Ok("Fix the flaky jobs test"));
    let title = ask(&namer, "the jobs test fails one run in ten, find out why");
    assert_eq!(title.as_deref(), Some("Fix the flaky jobs test"));
    let asked = namer.asked.lock().unwrap();
    let (conversation, tools, answer_only) = &asked[0];
    assert_eq!((asked.len(), *tools, *answer_only), (1, 0, true));
    let Message::User(said) = &conversation[1] else {
        panic!("{conversation:?}")
    };
    assert!(
        said.ends_with("User prompt:\nthe jobs test fails one run in ten, find out why"),
        "{said}"
    );
}

#[test]
fn a_failed_or_empty_answer_names_nothing() {
    assert_eq!(ask(&Namer::new(Err(())), "go"), None);
    assert_eq!(ask(&Namer::new(Ok("  \n")), "go"), None);
    assert_eq!(
        ask(&Namer::new(Ok("x")), "   "),
        None,
        "no task, no request"
    );
}

#[test]
fn an_answer_is_cleaned_to_one_short_line() {
    for (said, title) in [
        ("\"Fix the parser.\"", "Fix the parser"),
        ("  Add   tabs\tto the printer!  ", "Add tabs to the printer"),
        ("«Διόρθωση δοκιμών»", "Διόρθωση δοκιμών"),
        (
            "Title: Speed up the build\nbecause it is slow",
            "Speed up the build",
        ),
        ("`Rename the picker?`", "Rename the picker"),
        // What could redraw the screen is shown, not obeyed.
        ("Fix \u{1b}[2K \u{202e}txt", "Fix \\u{1b}[2K \\u{202e}txt"),
    ] {
        assert_eq!(clean(said).as_deref(), Some(title), "{said:?}");
    }
    let long = clean("Make every single one of the release binaries much smaller").unwrap();
    assert!(long.chars().count() <= MAX, "{long}");
    assert!(long.ends_with('…'), "{long}");
}

#[test]
fn a_long_task_is_cut_to_the_prompt_budget_on_a_character() {
    let task = "λ".repeat(2000);
    let prompt = prompt(&task);
    assert!(prompt.len() <= PROMPT_BYTES, "{}", prompt.len());
    assert!(prompt.ends_with('λ'));
}

/// The lane's end waits for a title still asked for: its request is held to a deadline.
#[test]
fn the_titles_request_is_given_up_within_a_minute() {
    let namer = Namer::new(Ok("Fix the parser"));
    assert!(ask(&namer, "fix the parser").is_some());
    let deadline = namer.deadlines.lock().unwrap()[0];
    let minute = Instant::now() + Duration::from_secs(60);
    assert!(deadline.is_some_and(|by| by <= minute), "{deadline:?}");
}

/// The title's thread is the lane's: let go at the lane's end, the request still out is waited
/// for there, so nothing of it outlives the lane (R2).
#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "a request that takes a moment, as a model's does"
)]
fn the_lanes_end_waits_for_the_title_still_asked_for() {
    let answered = Arc::new(AtomicBool::new(false));
    let theirs = Arc::clone(&answered);
    let mut titling = Titling::default();
    titling
        .start(move || {
            std::thread::sleep(Duration::from_millis(200));
            theirs.store(true, Ordering::SeqCst);
        })
        .unwrap();
    drop(titling);
    assert!(
        answered.load(Ordering::SeqCst),
        "the lane ended before its title"
    );
}

/// A task never waits for a title: the next one starts while one is still out, kept with it for
/// the lane's end.
#[test]
fn a_task_never_waits_for_the_title_still_asked_for() {
    let (release, held) = std::sync::mpsc::sync_channel::<()>(0);
    let mut titling = Titling::default();
    let slow = move || {
        let _ = held.recv_timeout(Duration::from_secs(5));
    };
    titling.start(slow).unwrap();
    let began = Instant::now();
    titling.start(|| {}).unwrap();
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "the next task waited"
    );
    drop(release);
}
