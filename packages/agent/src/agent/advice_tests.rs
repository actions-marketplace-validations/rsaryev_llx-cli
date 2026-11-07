use std::sync::Mutex;

use contracts::advice::{Advisor, Review, Task};
use contracts::catalog::Effort;
use contracts::chat::Message;
use contracts::tool::Tool;

use super::super::{Agent, Status};
use crate::fakes::{Echo, Keep, Scripted, Silent, calls, data, done, ok};

/// An advisor that says what it was set to, and writes down what it was asked.
#[derive(Default)]
struct Told {
    level: Option<Effort>,
    review: Option<String>,
    asked: Mutex<Vec<String>>,
    reviewed: Mutex<Vec<String>>,
}

impl Advisor for Told {
    fn level(&self, goal: &str) -> Option<Effort> {
        self.asked.lock().unwrap().push(goal.into());
        self.level
    }
    fn review(&self, _: &Task, answer: &str) -> Review {
        self.reviewed.lock().unwrap().push(answer.into());
        self.review.clone().map_or(Review::Done, Review::GoOn)
    }
}

fn agent<'a>(
    chat: &'a Scripted,
    tools: &'a [&'a dyn Tool],
    advisor: Option<&'a dyn Advisor>,
    max_turns: usize,
) -> Agent<'a> {
    Agent {
        chat,
        tools,
        context: "",
        max_turns,
        observer: &Silent,
        schema: None,
        inbox: None,
        window: 0,
        strategy: &Keep,
        advisor,
    }
}

fn sent(chat: &Scripted) -> Vec<(Vec<Message>, Option<Effort>)> {
    chat.sent.lock().unwrap().clone()
}

#[test]
fn an_advisor_with_nothing_to_say_changes_no_request() {
    let script = || Scripted::new(vec![calls("echo", serde_json::json!({})), done("Done.")]);
    let (alone, advised) = (script(), script());
    let (echo, again) = (Echo::new(vec![ok("ran")]), Echo::new(vec![ok("ran")]));
    agent(&alone, &[&echo], None, 5).run(None, "Do it", &[]);
    let quiet = Told::default();
    agent(&advised, &[&again], Some(&quiet), 5).run(None, "Do it", &[]);
    assert_eq!(sent(&alone), sent(&advised));
    assert_eq!(quiet.reviewed.lock().unwrap().as_slice(), ["Done."]);
}

#[test]
fn a_fresh_conversation_thinks_at_the_level_chosen_and_one_that_goes_on_is_not_asked() {
    let chat = Scripted::new(vec![done("One."), done("Two.")]);
    let low = Told {
        level: Some(Effort::Low),
        ..Told::default()
    };
    let run = agent(&chat, &[], Some(&low), 5).run(None, "First", &[]);
    // A follow-up or a fork goes on at its level: the cache stays whole.
    let high = Told {
        level: Some(Effort::High),
        ..Told::default()
    };
    agent(&chat, &[], Some(&high), 5).run(Some(run.conversation), "Second", &[]);
    let levels: Vec<_> = sent(&chat).into_iter().map(|(_, effort)| effort).collect();
    assert_eq!(levels, [Some(Effort::Low), Some(Effort::Low)]);
    assert_eq!(low.asked.lock().unwrap().as_slice(), ["First"]);
    assert!(high.asked.lock().unwrap().is_empty());
}

#[test]
fn an_answer_found_unfinished_goes_on_twice_at_most() {
    let chat = Scripted::new(vec![done("A."), done("B."), done("C.")]);
    let pushing = Told {
        review: Some("Run the tests first.".into()),
        ..Told::default()
    };
    let run = agent(&chat, &[], Some(&pushing), 5).run(None, "Do it", &[]);
    assert_eq!((run.status, run.answer.as_str()), (Status::Done, "C."));
    let last = &sent(&chat)[2].0;
    let pushed = (last.iter())
        .filter(|message| matches!(message, Message::User(text) if text == "Run the tests first."))
        .count();
    assert_eq!(pushed, 2);
}

#[test]
fn an_answer_on_the_last_turn_stands() {
    let chat = Scripted::new(vec![done("A."), done("B.")]);
    let pushing = Told {
        review: Some("Go on.".into()),
        ..Told::default()
    };
    let run = agent(&chat, &[], Some(&pushing), 2).run(None, "Do it", &[]);
    assert_eq!((run.status, run.answer.as_str()), (Status::Done, "B."));
    assert_eq!(pushing.reviewed.lock().unwrap().as_slice(), ["A."]);
}

#[test]
fn data_returned_before_a_push_on_is_not_kept_when_the_work_then_runs_out() {
    // The advisor found the answer unfinished: the data it came with is not the task's.
    let schema = serde_json::json!({"type": "object"});
    let chat = Scripted::new(vec![
        data(&serde_json::json!({"early": true})),
        calls("echo", serde_json::json!({})),
    ]);
    let echo = Echo::new(vec![ok("ran")]);
    let told = Told {
        review: Some("Check the tests too.".into()),
        ..Told::default()
    };
    let run = Agent {
        schema: Some(&schema),
        ..agent(&chat, &[&echo], Some(&told), 2)
    }
    .run(None, "Do it", &[]);
    assert_eq!(run.status, Status::Exhausted);
    assert_eq!(run.data, None);
}
