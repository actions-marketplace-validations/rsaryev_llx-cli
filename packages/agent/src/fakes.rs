//! Fakes of the ports the agent speaks to, for its tests: a scripted chat model, a scripted
//! tool, an observer that writes events down and one that shows nothing, and `Keep`, the history
//! strategy that sends a conversation as it is. No network, no real tool.

use std::sync::Mutex;

use contracts::catalog::Effort;
use contracts::chat::{
    Calling, Chat, ChatError, Completion, Message, Request, TaskId, ToolCall, Usage,
};
use contracts::tool::{Input, Tool, ToolError, ToolOutput, ToolSpec};
use serde_json::{Value, json};

use crate::conversation::Conversation;
use crate::history::{Report, Strategy};
use crate::observer::Observer;

/// A chat model that replies from a script, and remembers what each request allowed.
pub(crate) struct Scripted {
    pub(crate) replies: Mutex<Vec<Completion>>,
    /// What each request let the model call: `any`, `nothing`, or the one tool's name.
    pub(crate) calling: Mutex<Vec<String>>,
    /// The last user message of each request.
    pub(crate) last_user: Mutex<Vec<String>>,
    /// Each request's messages and level, as sent.
    pub(crate) sent: Mutex<Vec<(Vec<Message>, Option<Effort>)>>,
    /// The task each request named.
    pub(crate) tasks: Mutex<Vec<TaskId>>,
    /// The names of the tools each request offered.
    pub(crate) offered: Mutex<Vec<Vec<String>>>,
}

impl Scripted {
    pub(crate) fn new(replies: Vec<Completion>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().rev().collect()),
            calling: Mutex::new(vec![]),
            last_user: Mutex::new(vec![]),
            sent: Mutex::new(vec![]),
            tasks: Mutex::new(vec![]),
            offered: Mutex::new(vec![]),
        }
    }
}

impl Chat for Scripted {
    #[expect(
        clippy::unwrap_in_result,
        reason = "a test's fake: a poisoned lock fails the test"
    )]
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "a test picks out the variant it checks"
    )]
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        (self.sent.lock().unwrap()).push((request.conversation.to_vec(), request.effort));
        self.tasks.lock().unwrap().push(request.task);
        let names = request.tools.iter().map(|spec| spec.name.clone()).collect();
        self.offered.lock().unwrap().push(names);
        self.calling.lock().unwrap().push(match request.calling {
            Calling::Any => "any".into(),
            Calling::Nothing => "nothing".into(),
            Calling::Only(name) => name.into(),
        });
        let user = request
            .conversation
            .iter()
            .rev()
            .find_map(|message| match message {
                Message::User(text) => Some(text.clone()),
                _ => None,
            });
        self.last_user
            .lock()
            .unwrap()
            .push(user.unwrap_or_default());
        self.replies
            .lock()
            .unwrap()
            .pop()
            .ok_or_else(|| ChatError::Unavailable("the script ran out".into()))
    }
}

/// An observer that writes each event down as one line.
#[derive(Default)]
pub(crate) struct Recorder(Mutex<Vec<String>>);

impl Recorder {
    pub(crate) fn events(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

impl Observer for Recorder {
    fn call(&self, call: &ToolCall) {
        self.0.lock().unwrap().push(format!("call {}", call.name));
    }
    fn result(&self, call: &ToolCall, outcome: &Result<ToolOutput, ToolError>) {
        let line = match outcome {
            Ok(output) => format!("ok {}: {}", call.name, output.report),
            Err(ToolError::Failed(why)) => format!("failed {}: {why}", call.name),
            Err(ToolError::Fatal(why)) => format!("fatal {}: {why}", call.name),
        };
        self.0.lock().unwrap().push(line);
    }
    fn said(&self, text: &str) {
        self.0.lock().unwrap().push(format!("said {text}"));
    }
    fn turn(&self, usage: &Usage) {
        self.0
            .lock()
            .unwrap()
            .push(format!("turn {}", usage.output_tokens));
    }
}

/// The model says `text` and calls nothing: the answer, unless something makes the run go on.
pub(crate) fn says(text: &str) -> Completion {
    Completion {
        text: Some(text.into()),
        ..Completion::default()
    }
}

/// The model ends the task: a message with no call, its answer.
pub(crate) fn done(answer: &str) -> Completion {
    says(answer)
}

/// The `result` call of `--schema`, with `data`.
pub(crate) fn data(data: &Value) -> Completion {
    calls(crate::schema::TOOL, json!({ "data": data }))
}

/// The model calls `name` with `arguments`, as the call `c1`.
pub(crate) fn calls(name: &str, arguments: Value) -> Completion {
    Completion {
        calls: vec![ToolCall {
            id: "c1".into(),
            name: name.into(),
            arguments,
        }],
        ..Completion::default()
    }
}

/// A tool that answers each call from a script.
pub(crate) struct Echo(Mutex<Vec<Result<ToolOutput, ToolError>>>);

impl Echo {
    pub(crate) fn new(outcomes: Vec<Result<ToolOutput, ToolError>>) -> Self {
        Self(Mutex::new(outcomes.into_iter().rev().collect()))
    }
}

impl Tool for Echo {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "echo".into(),
            description: "Says what it did.".into(),
            input: Input::Json(json!({"type": "object"})),
        }
    }
    #[expect(
        clippy::unwrap_in_result,
        reason = "a test's fake: an unexpected call fails the test"
    )]
    fn call(&self, _: &Value) -> Result<ToolOutput, ToolError> {
        self.0.lock().unwrap().pop().expect("an unexpected call")
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "a scripted tool reply, which is a call's result"
)]
pub(crate) fn ok(report: &str) -> Result<ToolOutput, ToolError> {
    Ok(report.to_string().into())
}

/// What the person said, in order.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
pub(crate) fn users(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::User(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// What the tools answered, in order.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
pub(crate) fn tool_results(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult { content, .. } => Some(content.clone()),
            _ => None,
        })
        .collect()
}

/// Shows nothing.
pub(crate) struct Silent;

impl Observer for Silent {}

/// Sends the conversation as it is.
pub(crate) struct Keep;

impl Strategy for Keep {
    fn prepare(&self, _: &mut Conversation, _: u64, _: f64) -> Report {
        vec![]
    }
}

/// A `--schema` shape that checks what the agent's tests need of one: the fields a schema
/// requires and the types of its own fields, said as the app's check says them.
pub(crate) struct Shaped(pub(crate) Value);

impl contracts::shape::Shape for Shaped {
    fn schema(&self) -> &Value {
        &self.0
    }

    fn mismatches(&self, data: &Value) -> Vec<String> {
        let mut out = vec![];
        let required = self.0["required"].as_array().into_iter().flatten();
        for name in required.filter_map(Value::as_str) {
            if data.get(name).is_none() {
                out.push(format!("data is missing \"{name}\""));
            }
        }
        let typed = self.0["properties"].as_object().into_iter().flatten();
        for (name, rule) in typed {
            let kind = rule["type"].as_str().unwrap_or("any");
            let held = match (kind, data.get(name)) {
                (_, None) | ("any", _) => true,
                ("number", Some(value)) => value.is_number(),
                ("string", Some(value)) => value.is_string(),
                (_, Some(_)) => true,
            };
            if !held {
                out.push(format!("data.{name} must be {kind}"));
            }
        }
        out
    }
}
