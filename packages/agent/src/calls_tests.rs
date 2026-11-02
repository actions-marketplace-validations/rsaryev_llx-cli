use std::sync::{Condvar, Mutex};
use std::time::Duration;

use contracts::chat::{Completion, Message, ToolCall};
use contracts::tool::{Input, Tool, ToolError, ToolOutput, ToolSpec};
use serde_json::{Value, json};

use crate::agent::Agent;
use crate::fakes::Silent;
use crate::fakes::{Recorder, Scripted, done};

/// Where the calls of a test's tools run, whichever tool they belong to: how many run now,
/// the most that ever ran at once, and the order they started and ended in.
#[derive(Default)]
struct Room {
    running: Mutex<(usize, usize)>,
    changed: Condvar,
    log: Mutex<Vec<String>>,
}

impl Room {
    fn peak(&self) -> usize {
        self.running.lock().unwrap().1
    }

    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

/// A tool whose calls go through a room. With `meet` above one, a call waits (a few seconds
/// at most, and no longer than it takes) for that many calls to have run at once, so a test of
/// concurrency does not race the scheduler. Otherwise a call holds a moment, so that an overlap
/// with another would show in the room's peak.
struct Meet<'a> {
    room: &'a Room,
    parallel: bool,
    meet: usize,
}

fn look(room: &Room) -> Meet<'_> {
    Meet {
        room,
        parallel: true,
        meet: 1,
    }
}

impl Tool for Meet<'_> {
    fn spec(&self) -> ToolSpec {
        let name = if self.parallel { "look" } else { "change" };
        ToolSpec {
            name: name.into(),
            description: String::new(),
            input: Input::Json(json!({"type": "object"})),
        }
    }

    #[expect(
        clippy::unwrap_in_result,
        clippy::disallowed_methods,
        reason = "a test's fake tool: a poisoned lock fails the test, and a call alone works 20 ms so that calls could overlap"
    )]
    fn call(&self, arguments: &Value) -> Result<ToolOutput, ToolError> {
        let id = arguments["id"].as_str().unwrap_or("").to_string();
        let room = self.room;
        let mut counts = room.running.lock().unwrap();
        counts.0 += 1;
        counts.1 = counts.1.max(counts.0);
        room.log.lock().unwrap().push(format!("start {id}"));
        room.changed.notify_all();
        if self.meet > 1 {
            counts = room
                .changed
                .wait_timeout_while(counts, Duration::from_secs(5), |(_, peak)| {
                    *peak < self.meet
                })
                .unwrap()
                .0;
        } else {
            drop(counts);
            std::thread::sleep(Duration::from_millis(20));
            counts = room.running.lock().unwrap();
        }
        let together = counts.1 >= 2;
        counts.0 -= 1;
        room.log.lock().unwrap().push(format!("end {id}"));
        drop(counts);
        Ok(ToolOutput {
            report: format!("{id} {}", if together { "together" } else { "alone" }),
            ..ToolOutput::default()
        })
    }

    fn parallel(&self) -> bool {
        self.parallel
    }
}

fn call(id: &str, name: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: name.into(),
        arguments: json!({"id": id}),
    }
}

fn turn(calls: Vec<ToolCall>) -> Completion {
    Completion {
        calls,
        ..Completion::default()
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
fn results(messages: &[Message]) -> Vec<(String, String)> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult { id, content, .. } => Some((id.clone(), content.clone())),
            _ => None,
        })
        .collect()
}

fn agent<'a>(chat: &'a Scripted, tools: &'a [&'a dyn Tool]) -> Agent<'a> {
    Agent {
        chat,
        tools,
        context: "",
        max_turns: 6,
        observer: &Silent,
        schema: None,
        inbox: None,
        window: 0,
        strategy: &crate::fakes::Keep,
        advisor: None,
    }
}

#[test]
fn calls_that_may_run_together_do_and_answer_in_the_models_order() {
    let room = Room::default();
    let look = Meet {
        meet: 2,
        ..look(&room)
    };
    let chat = Scripted::new(vec![
        turn(vec![call("a", "look"), call("b", "look")]),
        done("Done."),
    ]);
    let tools: [&dyn Tool; 1] = [&look];
    let run = agent(&chat, &tools).run(None, "look twice", &[]);
    assert_eq!(room.peak(), 2, "both ran at once");
    let results = results(run.conversation.messages());
    assert_eq!(
        results[..2],
        [
            ("a".to_string(), "a together".to_string()),
            ("b".to_string(), "b together".to_string())
        ]
    );
}

#[test]
fn a_call_that_may_not_runs_alone_between_the_others() {
    let room = Room::default();
    let change = Meet {
        parallel: false,
        ..look(&room)
    };
    let look = look(&room);
    let chat = Scripted::new(vec![
        turn(vec![
            call("a", "look"),
            call("x", "change"),
            call("b", "look"),
        ]),
        done("Done."),
    ]);
    let tools: [&dyn Tool; 2] = [&look, &change];
    let run = agent(&chat, &tools).run(None, "look, change, look", &[]);
    assert_eq!(room.peak(), 1, "no two calls overlapped");
    assert_eq!(
        room.log(),
        ["start a", "end a", "start x", "end x", "start b", "end b"]
    );
    let results: Vec<(String, String)> = results(run.conversation.messages());
    assert_eq!(
        results[..3],
        [
            ("a".to_string(), "a alone".to_string()),
            ("x".to_string(), "x alone".to_string()),
            ("b".to_string(), "b alone".to_string())
        ]
    );
}

#[test]
fn a_batch_is_watched_call_by_call() {
    let room = Room::default();
    let look = look(&room);
    let chat = Scripted::new(vec![
        turn(vec![call("a", "look"), call("b", "look")]),
        done("Done."),
    ]);
    let recorder = Recorder::default();
    let tools: [&dyn Tool; 1] = [&look];
    let agent = Agent {
        observer: &recorder,
        ..agent(&chat, &tools)
    };
    agent.run(None, "look", &[]);
    let events: Vec<String> = recorder
        .events()
        .into_iter()
        .filter(|event| !event.starts_with("turn"))
        .collect();
    assert_eq!(events[..2], ["call look", "call look"]);
    assert_eq!(
        events.iter().filter(|e| e.starts_with("ok look")).count(),
        2
    );
}

/// A tool that takes free text and reports it back.
struct Script;

impl Tool for Script {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "script".into(),
            description: String::new(),
            input: Input::Text("start: /.+/".into()),
        }
    }

    fn call(&self, arguments: &Value) -> Result<ToolOutput, ToolError> {
        let text = arguments.as_str().unwrap_or("not text");
        Ok(ToolOutput::from(format!("ran {text}")))
    }
}

#[test]
fn a_text_tool_is_called_with_its_text_and_a_json_tool_refuses_text() {
    let room = Room::default();
    let look = look(&room);
    let chat = Scripted::new(vec![
        turn(vec![
            ToolCall {
                id: "s".into(),
                name: "script".into(),
                arguments: json!("return 1"),
            },
            ToolCall {
                id: "o".into(),
                name: "script".into(),
                arguments: json!({"source": "return 1"}),
            },
        ]),
        turn(vec![ToolCall {
            id: "l".into(),
            name: "look".into(),
            arguments: json!("path"),
        }]),
        done("Done."),
    ]);
    let tools: [&dyn Tool; 2] = [&Script, &look];
    let run = agent(&chat, &tools).run(None, "go", &[]);
    let results = results(run.conversation.messages());
    assert_eq!(results[0], ("s".to_string(), "ran return 1".to_string()));
    assert!(results[1].1.contains("takes text"), "{results:?}");
    assert!(results[2].1.contains("JSON object"), "{results:?}");
}
