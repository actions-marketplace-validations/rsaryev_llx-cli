//! A scripted model behind a real HTTP server: the binary under test calls it as it calls
//! OpenRouter (`OPENROUTER_BASE_URL`), it answers each chat completion from a script, and it keeps
//! every request for the test to read. No network. A session's title is asked for beside its
//! tasks: those requests are answered apart (with the title set, else the task itself), never from
//! the script, and kept apart. So is the advice a task asks of the decision model (`/systemone`):
//! answered with nothing, so it changes nothing, unless the script's next reply is a decision, and
//! kept apart too (a scripted decision is kept among the requests as well).

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use super::eventually;

/// One answer of the model: text, a tool call, or an HTTP error with its status.
pub(crate) enum Reply {
    Text(String),
    Call(String, Value),
    Refused(u16),
    /// The reply given once the file exists: a lane's request waits while another works.
    After(std::path::PathBuf, Box<Reply>),
    /// A decision model's answer to `/systemone`: the key it picked, with each key's probability.
    Decided(String, Vec<(String, f64)>),
    /// A reply the output limit cut off while it wrote a call to `name`: its arguments as far as
    /// they got (not JSON).
    Cut(String, String),
}

impl Reply {
    /// The model calls `name` with `arguments`.
    pub(crate) fn call(name: &str, arguments: Value) -> Self {
        Self::Call(name.into(), arguments)
    }

    /// The model ends the task: a message with no call, its answer.
    pub(crate) fn done(answer: &str) -> Self {
        Self::Text(answer.into())
    }

    /// A decision model's answer: `choice`, with each key's probability in `odds`.
    pub(crate) fn decided(choice: &str, odds: &[(&str, f64)]) -> Self {
        let odds = odds.iter().map(|(key, p)| ((*key).to_string(), *p));
        Self::Decided(choice.into(), odds.collect())
    }

    /// The output limit cuts the reply off while it calls `name`, at `written` of its arguments.
    pub(crate) fn cut(name: &str, written: &str) -> Self {
        Self::Cut(name.into(), written.into())
    }

    /// The `result` call of `--schema`, with `data`.
    pub(crate) fn data(data: &Value) -> Self {
        Self::call("result", json!({ "data": data }))
    }

    /// The model makes the file `path` holding `text`, with a patch.
    pub(crate) fn create(path: &str, text: &str) -> Self {
        let lines: String = text.lines().flat_map(|line| ["+", line, "\n"]).collect();
        let patch = format!("*** Begin Patch\n*** Add File: {path}\n{lines}*** End Patch");
        Self::call("apply_patch", json!({ "source": patch }))
    }
}

#[derive(Default)]
struct Shared {
    script: VecDeque<Reply>,
    requests: Vec<Value>,
    /// The title a title's request is answered with; `None` echoes the task.
    title: Option<String>,
    /// The tasks titles were asked for.
    titled: Vec<String>,
    /// Every decision asked of `/systemone`, scripted or refused.
    advised: Vec<Value>,
}

/// How the title's request starts (`meta::title::INSTRUCTIONS`), its task after `User prompt:\n`:
/// both mirror `apps/server/src/meta/title.rs`, and change with it.
const TITLE: &str = "Generate a concise, single-line task title";

/// The server; it stops with the test process.
pub(crate) struct Model {
    pub(crate) base: String,
    shared: Arc<Mutex<Shared>>,
}

impl Model {
    /// A model that answers `script` in order, then "Done." to anything more.
    pub(crate) fn start(script: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let shared = Arc::new(Mutex::new(Shared {
            script: script.into(),
            ..Shared::default()
        }));
        let serving = Arc::clone(&shared);
        let server = std::thread::Builder::new().name("model".into());
        server
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let shared = Arc::clone(&serving);
                    let answering = std::thread::Builder::new().name("model-answer".into());
                    answering.spawn(move || answer(stream, &shared)).unwrap();
                }
            })
            .unwrap();
        Self { base, shared }
    }

    /// The same, answering a title's request with `title`.
    pub(crate) fn titling(script: Vec<Reply>, title: &str) -> Self {
        let model = Self::start(script);
        model.shared.lock().unwrap().title = Some(title.into());
        model
    }

    /// The tasks a title was asked for, in order.
    pub(crate) fn titled(&self) -> Vec<String> {
        self.shared.lock().unwrap().titled.clone()
    }

    /// Every decision the decision model was asked, as JSON, in order.
    pub(crate) fn advised(&self) -> Vec<Value> {
        self.shared.lock().unwrap().advised.clone()
    }

    /// Every request the model received but the titles' and the advice it answered with nothing,
    /// as JSON, in order.
    pub(crate) fn requests(&self) -> Vec<Value> {
        self.shared.lock().unwrap().requests.clone()
    }
}

fn answer(stream: TcpStream, shared: &Mutex<Shared>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let mut length = 0;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; length];
    let _ = reader.read_exact(&mut body);
    let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if content(&request["messages"][0]).starts_with(TITLE) {
        let asked = content(&request["messages"][1]);
        let task = asked
            .split_once("User prompt:\n")
            .map_or("", |(_, task)| task);
        let title = {
            let mut shared = shared.lock().unwrap();
            shared.titled.push(task.to_string());
            shared.title.clone().unwrap_or_else(|| task.to_string())
        };
        return respond(
            stream,
            200,
            &completion(&json!({"role": "assistant", "content": title}), 0, &request),
        );
    }
    if request.get("questions").is_some() {
        let mut held = shared.lock().unwrap();
        held.advised.push(request.clone());
        if !matches!(held.script.front(), Some(Reply::Decided(..))) {
            drop(held);
            let none = json!({"model": request["model"], "answers": {}, "usage": {"cost": 0.0}});
            return respond(stream, 200, &none);
        }
    }
    let (mut next, n) = {
        let mut shared = shared.lock().unwrap();
        shared.requests.push(request.clone());
        (shared.script.pop_front(), shared.requests.len())
    };
    let (status, body) = loop {
        let message = match next {
            Some(Reply::After(file, reply)) => {
                let (wait, every) = (Duration::from_secs(30), Duration::from_millis(20));
                let _ = eventually(wait, every, || file.exists().then_some(()));
                next = Some(*reply);
                continue;
            }
            Some(Reply::Refused(status)) => {
                let error = json!({"code": status, "message": "No auth credentials found"});
                break (status, json!({ "error": error }));
            }
            Some(Reply::Decided(choice, probabilities)) => {
                break (200, decision(&request, &choice, &probabilities));
            }
            Some(Reply::Text(text)) => json!({"role": "assistant", "content": text}),
            Some(Reply::Call(name, arguments)) => {
                let call = json!({"id": format!("call_{n}"), "type": "function",
                    "function": {"name": name, "arguments": arguments.to_string()}});
                json!({"role": "assistant", "content": null, "tool_calls": [call]})
            }
            Some(Reply::Cut(name, written)) => {
                let call = json!({"id": format!("call_{n}"), "type": "function",
                    "function": {"name": name, "arguments": written}});
                let message = json!({"role": "assistant", "content": null, "tool_calls": [call]});
                let mut body = completion(&message, n, &request);
                body["choices"][0]["finish_reason"] = json!("length");
                break (200, body);
            }
            // Past the script: the task ends with an answer.
            None => json!({"role": "assistant", "content": "Done."}),
        };
        break (200, completion(&message, n, &request));
    };
    if status == 200 && request["stream"] == json!(true) {
        return respond_streamed(stream, &body);
    }
    respond(stream, status, &body);
}

/// A message's text: a string, or the text of its parts (a cache mark makes it parts).
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
fn content(message: &Value) -> String {
    match &message["content"] {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part["text"].as_str())
            .collect(),
        _ => String::new(),
    }
}

fn respond(mut stream: TcpStream, status: u16, body: &Value) {
    let text = body.to_string();
    let _ = write!(
        stream,
        "HTTP/1.1 {status} Status\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{text}",
        text.len()
    );
}

/// `completion` as OpenRouter streams it: its text and each call's arguments in two pieces, the
/// usage and the reason it ended in the last event.
fn respond_streamed(mut stream: TcpStream, completion: &Value) {
    let message = &completion["choices"][0]["message"];
    let halves = |text: &str| {
        let at = text
            .char_indices()
            .nth(text.chars().count() / 2)
            .map_or(0, |(at, _)| at);
        [text[..at].to_string(), text[at..].to_string()]
    };
    let event = |delta: Value| json!({"model": completion["model"], "choices": [{"index": 0, "delta": delta}]});
    let mut events = vec![];
    if let Some(text) = message["content"].as_str() {
        events.extend(halves(text).map(|part| event(json!({"content": part}))));
    }
    for (index, call) in message["tool_calls"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let [first, rest] = halves(call["function"]["arguments"].as_str().unwrap_or(""));
        events.push(event(json!({"tool_calls": [{"index": index, "id": call["id"],
            "type": "function", "function": {"name": call["function"]["name"], "arguments": first}}]})));
        events.push(event(json!({"tool_calls": [{"index": index,
            "function": {"arguments": rest}}]})));
    }
    let called = if message["tool_calls"].is_array() {
        "tool_calls"
    } else {
        "stop"
    };
    let reason = completion["choices"][0]["finish_reason"]
        .as_str()
        .unwrap_or(called);
    events.push(
        json!({"model": completion["model"], "usage": completion["usage"],
        "choices": [{"index": 0, "delta": {}, "finish_reason": reason}]}),
    );
    let mut body = String::from(": OPENROUTER PROCESSING\n\n");
    for event in events {
        let _ = std::fmt::Write::write_fmt(&mut body, format_args!("data: {event}\n\n"));
    }
    body.push_str("data: [DONE]\n\n");
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
}

/// The chat completion carrying `message` (the `n`th request).
fn completion(message: &Value, n: usize, request: &Value) -> Value {
    let model = request["model"].as_str().unwrap_or("test/model");
    json!({"id": format!("c{n}"), "model": model, "choices": [{"index": 0, "message": message}],
        "usage": {"prompt_tokens": 1000, "completion_tokens": 20, "cost": 0.0001}})
}

/// `/systemone`'s answer: every question asked gets `choice`, with `probabilities`.
fn decision(request: &Value, choice: &str, probabilities: &[(String, f64)]) -> Value {
    let odds: serde_json::Map<String, Value> = (probabilities.iter())
        .map(|(key, p)| (key.clone(), json!(p)))
        .collect();
    let answers: serde_json::Map<String, Value> = (request["questions"].as_object().unwrap())
        .keys()
        .map(|key| {
            let answer = json!({"type": "choice", "choice": choice, "probabilities": odds,
                "confidence": 0.9});
            (key.clone(), answer)
        })
        .collect();
    json!({"model": request["model"], "answers": answers,
        "usage": {"cost": 0.00002, "input_tokens": 300, "output_tokens": 1}})
}

/// A task that runs `command`, then answers.
pub(crate) fn running(command: &str) -> Model {
    Model::start(vec![
        Reply::call("bash", json!({"command": command})),
        Reply::done("Done."),
    ])
}

/// What the model was asked in its `at`th request, as one text: its messages' contents.
pub(crate) fn said(model: &Model, at: usize) -> String {
    model.requests()[at]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["content"].to_string())
        .collect::<Vec<_>>()
        .join("\n")
}
