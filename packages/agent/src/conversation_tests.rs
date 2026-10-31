use contracts::chat::ToolCall;
use serde_json::json;

use super::*;
use crate::fakes::tool_results;

#[test]
fn a_result_too_long_keeps_its_head_and_tail() {
    let text = format!("{}{}", "a".repeat(MAX_RESULT), "b".repeat(10));
    let short = shortened(&text);
    assert!(short.starts_with("aaa") && short.ends_with("bbbbbbbbbb"));
    assert!(
        short.contains("… 10 characters cut …"),
        "{}",
        short
            .get(MAX_RESULT / 2..MAX_RESULT / 2 + 40)
            .unwrap_or(&short)
    );
    assert_eq!(shortened("short"), "short");
}

#[expect(clippy::format_collect, reason = "a test's fixture, built once")]
#[test]
fn a_whole_command_report_comes_through_unshortened() {
    // About what a command's two streams take at most: 80 KB (some 20,000 tokens) and a header.
    let lines: String = (1..=8000)
        .map(|n| format!("{n:>6}\t{}\n", "x".repeat(2)))
        .collect();
    let report = format!("exit 0 in 1.0s\n{lines}{}", "y".repeat(3000));
    assert!(report.chars().count() > 80_000);
    let mut conversation = Conversation::new("rules");
    conversation.result("1", &report);
    assert_eq!(tool_results(conversation.messages()), [report]);
}

#[test]
fn a_conversation_taken_over_answers_the_calls_it_left_open_and_is_measured() {
    let call = |id: &str| ToolCall {
        id: id.into(),
        name: "echo".into(),
        arguments: json!({}),
    };
    let taken = vec![
        Message::System("rules".into()),
        Message::User("Goal: look".into()),
        Message::Assistant {
            text: None,
            calls: vec![call("a"), call("b")],
            carried: vec![],
        },
        Message::ToolResult {
            id: "a".into(),
            content: "done".into(),
            failed: false,
        },
    ];
    let conversation = Conversation::of(taken.clone(), "yours now");
    let messages = conversation.messages();
    assert_eq!(&messages[..4], &taken[..]);
    assert_eq!(
        messages[4..],
        [Message::ToolResult {
            id: "b".into(),
            content: "yours now".into(),
            failed: false,
        }]
    );
    assert_eq!(
        conversation.tokens(),
        messages.iter().map(fit::tokens).sum::<u64>()
    );
}

#[test]
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
fn calls_left_unanswered_are_closed_before_the_conversation_goes_on() {
    let mut conversation = Conversation::new("rules");
    let call = |id: &str| ToolCall {
        id: id.into(),
        name: "echo".into(),
        arguments: json!({}),
    };
    conversation.push(Message::Assistant {
        text: None,
        calls: vec![call("a"), call("b")],
        carried: vec![],
    });
    conversation.failure("a", "refused");
    conversation.close_open_calls();
    conversation.close_open_calls();
    let answered: Vec<&str> = conversation
        .messages()
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(answered, ["a", "b"]);
}

#[test]
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
fn a_picture_the_person_attached_follows_their_message_and_is_never_dropped() {
    let mut conversation = Conversation::new("rules");
    let goal = Message::User("Goal: make it look like this".into());
    conversation.push(goal.clone());
    let image = Image {
        media_type: "image/png".into(),
        data: "iVBORw0KGgo=".into(),
        width: 640,
        height: 480,
    };
    conversation.attach("mockup.png", image.clone());
    let attached = |conversation: &Conversation| {
        let messages = conversation.messages();
        let at = messages
            .iter()
            .position(|message| *message == goal)
            .unwrap();
        match &messages[at + 1] {
            Message::Image {
                caption,
                image: kept,
            } => caption.contains("mockup.png") && *kept == image,
            _ => false,
        }
    };
    assert!(attached(&conversation), "it follows the person's message");
    for n in 1..=4 {
        conversation.result(&n.to_string(), "read");
    }
    assert!(attached(&conversation), "later results leave it there");
}

#[test]
fn the_token_estimate_follows_every_change_of_the_messages() {
    // The estimate is kept beside the messages: it must say what a fresh count says after a
    // push, an elision and a rewind.
    let fresh = |c: &Conversation| c.messages().iter().map(crate::history::tokens).sum::<u64>();
    let mut c = Conversation::new("rules");
    c.push(Message::User("Goal: x".into()));
    let call = ToolCall {
        id: "c1".into(),
        name: "echo".into(),
        arguments: json!({"text": "x".repeat(5_000)}),
    };
    c.push(Message::Assistant {
        text: None,
        calls: vec![call],
        carried: vec![],
    });
    c.result("c1", &"y".repeat(5_000));
    assert_eq!(c.tokens(), fresh(&c));
    assert_eq!(c.elide(2), 1);
    assert_eq!(c.elide(3), 1);
    assert_eq!(c.tokens(), fresh(&c));
    c.result("c1", "r");
    assert_eq!(c.tokens(), fresh(&c));
    c.truncate(2);
    assert_eq!(c.tokens(), fresh(&c));
}
