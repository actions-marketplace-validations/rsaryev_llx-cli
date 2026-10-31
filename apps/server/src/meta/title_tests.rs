use std::sync::Mutex;

use contracts::chat::{Chat, ChatError, Completion, Message, Request};

use super::*;

/// A chat model that answers `text` and keeps what it was asked.
struct Namer {
    text: Result<String, ()>,
    asked: Mutex<Vec<(Vec<Message>, usize, bool)>>,
}

impl Namer {
    fn new(text: Result<&str, ()>) -> Self {
        Self {
            text: text.map(str::to_string),
            asked: Mutex::new(vec![]),
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
