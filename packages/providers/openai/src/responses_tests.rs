use std::sync::Arc;

use contracts::catalog::{Effort, ModelEntry, Selection};
use provider_kit::http::Reply;
use serde_json::{Value, json};

use contracts::chat::{Calling, Message, TaskId};
use contracts::tool::{Input, ToolSpec};

use super::offer::Offer;
use super::request::items;
use super::*;
use crate::testing::{Scripted, access};
use crate::tokens::Keep;

/// A model as the catalog gives it: `gpt-6-sol` takes levels, another none.
fn entry(id: &str) -> ModelEntry {
    let efforts = match id {
        "gpt-6-sol" => vec![
            Effort::Off,
            Effort::Low,
            Effort::Medium,
            Effort::High,
            Effort::XHigh,
            Effort::Max,
        ],
        _ => vec![],
    };
    ModelEntry {
        id: id.into(),
        name: id.into(),
        default: efforts.get(2).copied().unwrap_or(Effort::Off),
        efforts,
        context: 400_000,
        output: 0,
        marks: false,
    }
}

/// The request body as `complete` builds it, its tools offered as `texts` says.
fn request_body(selection: &Selection, request: Request, texts: Texts) -> Value {
    let offer = Offer::new(request.tools, texts, request.conversation);
    let session = route::session(request.conversation);
    body(selection, request, &offer, &session)
}

const APP: App = App {
    name: "llx",
    slug: "llx",
    version: "0",
    site: "https://llx.sh",
};

/// A store that keeps tokens in memory and counts the saves.
#[derive(Default)]
struct Memory {
    kept: Mutex<Option<Tokens>>,
}

impl Keep for Arc<Memory> {
    fn load(&self) -> Option<Tokens> {
        self.kept.lock().unwrap().clone()
    }

    fn save(&self, tokens: &Tokens) {
        *self.kept.lock().unwrap() = Some(tokens.clone());
    }
}

/// An `Http` the test keeps a handle on.
struct Shared(Arc<Scripted>);

impl Http for Shared {
    fn post(
        &self,
        a: &str,
        h: &[(&str, &str)],
        b: &str,
        d: Option<Instant>,
    ) -> Result<Reply, HttpError> {
        self.0.post(a, h, b, d)
    }
}

fn tokens(expires: u64) -> Tokens {
    Tokens {
        access: access("acct-1", expires),
        refresh: "r1".into(),
        account: "acct-1".into(),
        expires,
    }
}

const LATER: u64 = 9_999_999_999;

fn model(replies: Vec<(u16, String)>, held: Tokens) -> (ChatModel, Arc<Scripted>, Arc<Memory>) {
    model_of(Scripted::new(replies), held)
}

fn model_of(http: Scripted, held: Tokens) -> (ChatModel, Arc<Scripted>, Arc<Memory>) {
    let http = Arc::new(http);
    let memory = Arc::new(Memory::default());
    let selection = Selection::new(entry("gpt-6-sol"));
    // One script for the backend and the token endpoint: a refresh and the reply, in order.
    let model = ChatModel::on(
        Box::new(Shared(Arc::clone(&http))),
        Box::new(Shared(Arc::clone(&http))),
        Box::new(Arc::clone(&memory)),
        APP,
        held,
        selection,
    );
    let model = ChatModel {
        pause: Duration::ZERO,
        ..model
    };
    (model, http, memory)
}

#[expect(clippy::format_collect, reason = "a test's fixture, built once")]
fn streamed(events: &[Value]) -> String {
    events
        .iter()
        .map(|event| format!("event: x\ndata: {event}\n\n"))
        .collect()
}

fn completed() -> Value {
    json!({"type": "response.completed", "response": {"model": "gpt-6-sol",
        "usage": {"input_tokens": 10, "input_tokens_details": {"cached_tokens": 4}, "output_tokens": 3}}})
}

fn conversation() -> Vec<Message> {
    vec![
        Message::System("rules".into()),
        Message::User("goal".into()),
        Message::Assistant {
            text: Some("looking".into()),
            calls: vec![ToolCall {
                id: "c1".into(),
                name: "read".into(),
                arguments: json!({"path": "a"}),
            }],
            carried: vec![],
        },
        Message::ToolResult {
            id: "c1".into(),
            content: "missing".into(),
            failed: true,
        },
    ]
}

#[test]
fn a_conversation_goes_out_as_input_items_with_the_level_chosen() {
    let spec = ToolSpec {
        name: "read".into(),
        description: "Read".into(),
        input: Input::Json(json!({"type": "object"})),
    };
    let conversation = conversation();
    let request = Request {
        conversation: &conversation,
        tools: std::slice::from_ref(&spec),
        calling: Calling::Any,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    let selection = Selection::new(entry("gpt-6-sol"))
        .with_effort(Effort::High)
        .unwrap();
    let body = request_body(&selection, request, Texts::Functions);
    assert_eq!(body["model"], "gpt-6-sol");
    assert_eq!(
        (body["store"].clone(), body["stream"].clone()),
        (json!(false), json!(true))
    );
    assert_eq!(body["reasoning"], json!({"effort": "high"}));
    let input = body["input"].as_array().unwrap();
    assert_eq!(input[0]["role"], "developer");
    assert_eq!(
        input[1]["content"][0],
        json!({"type": "input_text", "text": "goal"})
    );
    assert_eq!(input[2]["content"][0]["type"], "output_text");
    assert_eq!(
        input[3],
        json!({"type": "function_call", "call_id": "c1", "name": "read",
        "arguments": "{\"path\":\"a\"}"})
    );
    assert_eq!(
        input[4],
        json!({"type": "function_call_output", "call_id": "c1",
        "output": "Error: missing"})
    );
    assert_eq!(body["tools"][0]["name"], "read");
    assert_eq!(
        (
            body["tool_choice"].clone(),
            body["parallel_tool_calls"].clone()
        ),
        (json!("auto"), json!(true))
    );

    // Off is asked for by name: left out, the backend would reason at its own default.
    let off = Selection::new(entry("gpt-6-sol"))
        .with_effort(Effort::Off)
        .unwrap();
    let request = Request {
        conversation: &[],
        tools: &[],
        calling: Calling::Any,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    assert_eq!(
        request_body(&off, request, Texts::Functions)["reasoning"],
        json!({"effort": "none"})
    );

    // The `result` call with `--schema`'s data is asked for by name: the reply is that call.
    let only = Request {
        conversation: &[],
        tools: std::slice::from_ref(&spec),
        calling: Calling::Only("result"),
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    assert_eq!(
        request_body(&off, only, Texts::Functions)["tool_choice"],
        json!({"type": "function", "name": "result"})
    );

    // A model with no level is sent none, and a request with no tools names none.
    let bare = Request {
        conversation: &[],
        tools: &[],
        calling: Calling::Nothing,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    let body = request_body(&Selection::new(entry("gpt-next")), bare, Texts::Functions);
    assert!(body.get("reasoning").is_none() && body.get("tools").is_none());
}

#[test]
fn a_streamed_reply_becomes_text_calls_and_usage() {
    let reply = streamed(&[
        json!({"type": "response.output_text.delta", "delta": "ignored"}),
        json!({"type": "response.output_item.done", "item": {"type": "reasoning"}}),
        json!({"type": "response.output_item.done", "item": {"type": "message",
            "content": [{"type": "output_text", "text": " Done. "}]}}),
        json!({"type": "response.output_item.done", "item": {"type": "function_call",
            "call_id": "c2", "name": "edit", "arguments": "{\"path\": \"a"}}),
        completed(),
    ]);
    let completion = read(&reply, "fallback").unwrap();
    assert_eq!(completion.text.as_deref(), Some("Done."));
    assert_eq!(completion.calls[0].id, "c2");
    assert_eq!(
        completion.calls[0].arguments,
        json!("{\"path\": \"a"),
        "cut arguments stay text"
    );
    let usage = &completion.usage;
    assert_eq!(
        (usage.input_tokens, usage.cached_tokens, usage.output_tokens),
        (10, 4, 3)
    );
    assert_eq!(usage.model, "gpt-6-sol");
}

#[test]
fn a_failed_or_cut_stream_is_an_error() {
    let failed = streamed(&[
        json!({"type": "response.failed", "response": {"error": {"message": "overloaded"}}}),
    ]);
    assert!(
        matches!(read(&failed, "m"), Err(ChatError::Unavailable(m)) if m.contains("overloaded"))
    );
    let cut = streamed(&[json!({"type": "response.created"})]);
    assert!(matches!(read(&cut, "m"), Err(ChatError::Invalid(_))));
}

#[test]
fn a_request_names_the_account_and_the_token() {
    let (model, http, _) = model(vec![(200, streamed(&[completed()]))], tokens(LATER));
    let conversation = conversation();
    let request = Request {
        conversation: &conversation,
        tools: &[],
        calling: Calling::Nothing,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    model.complete(request).unwrap();
    let sent = &http.sent()[0];
    assert_eq!(
        sent.address,
        "https://chatgpt.com/backend-api/codex/responses"
    );
    assert_eq!(sent.header("chatgpt-account-id"), Some("acct-1"));
    assert_eq!(
        sent.header("authorization"),
        Some(format!("Bearer {}", access("acct-1", LATER)).as_str())
    );
    assert_eq!(sent.header("originator"), Some("llx"));
    assert_eq!(sent.header("accept"), Some("text/event-stream"));
}

#[test]
fn a_token_about_to_lapse_is_refreshed_and_kept_first() {
    let refreshed = json!({"access_token": access("acct-1", LATER), "refresh_token": "r2"});
    let replies = vec![
        (200, refreshed.to_string()),
        (200, streamed(&[completed()])),
    ];
    let (model, http, memory) = model(replies, tokens(1));
    let request = Request {
        conversation: &[],
        tools: &[],
        calling: Calling::Nothing,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    model.complete(request).unwrap();
    let sent = http.sent();
    assert_eq!(sent[0].address, "https://auth.openai.com/oauth/token");
    assert_eq!(
        sent[1].header("authorization"),
        Some(format!("Bearer {}", access("acct-1", LATER)).as_str())
    );
    assert_eq!(memory.load().map(|t| t.refresh), Some("r2".into()));
}

#[test]
fn threads_that_find_the_token_stale_refresh_it_once() {
    let refreshed = json!({"access_token": access("acct-1", LATER), "refresh_token": "r2"});
    let (model, http, _) = model(vec![(200, refreshed.to_string())], tokens(1));
    let got: Vec<Tokens> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|i| {
                std::thread::Builder::new()
                    .name(format!("refresh-{i}"))
                    .spawn_scoped(scope, || model.tokens(Renew::IfStale, None).unwrap())
                    .unwrap()
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(http.sent().len(), 1, "one refresh for all of them");
    assert!(got.iter().all(|t| t.refresh == "r2"));
}

#[test]
fn a_refused_token_is_refreshed_once_then_the_refusal_is_said() {
    let refreshed = json!({"access_token": access("acct-1", LATER), "refresh_token": "r2"});
    let replies = vec![
        (401, "{}".into()),
        (200, refreshed.to_string()),
        (401, r#"{"detail": "nope"}"#.into()),
    ];
    let (model, http, _) = model(replies, tokens(LATER));
    let request = Request {
        conversation: &[],
        tools: &[],
        calling: Calling::Nothing,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    let refused = model.complete(request).unwrap_err().to_string();
    assert!(
        refused.contains("nope") && refused.contains("/login"),
        "{refused}"
    );
    assert_eq!(http.sent().len(), 3, "one refresh, no loop");
}

#[test]
fn a_usage_limit_says_so() {
    let (model, _, _) = model(
        vec![(429, r#"{"error": {"message": "try later"}}"#.into())],
        tokens(LATER),
    );
    let request = Request {
        conversation: &[],
        tools: &[],
        calling: Calling::Nothing,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    let refused = model.complete(request).unwrap_err().to_string();
    assert!(refused.contains("usage limit") && refused.contains("try later"));
}

#[test]
fn a_picture_goes_in_as_a_user_message_with_an_input_image() {
    let image = contracts::tool::Image {
        media_type: "image/jpeg".into(),
        data: "/9j/AA==".into(),
        width: 2,
        height: 1,
    };
    let item = &items(
        &Message::Image {
            caption: "The screenshot".into(),
            image,
        },
        "gpt-6-sol",
        &Offer::new(&[], Texts::Functions, &[]),
    )[0];
    assert_eq!(item["role"], "user");
    assert_eq!(item["content"][0]["text"], "The screenshot");
    assert_eq!(item["content"][1]["type"], "input_image");
    assert_eq!(
        item["content"][1]["image_url"],
        "data:image/jpeg;base64,/9j/AA=="
    );
}

#[test]
fn a_stream_that_went_out_unanswered_is_sent_once_more_and_no_further() {
    let request = Request {
        conversation: &[],
        tools: &[],
        calling: Calling::Nothing,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    let (model, http, _) = model_of(
        Scripted::answering(vec![
            Err(HttpError::Unanswered("socket dropped".into())),
            Ok(Reply {
                status: 200,
                headers: vec![],
                body: streamed(&[completed()]),
            }),
        ]),
        tokens(LATER),
    );
    assert!(model.complete(request).is_ok());
    assert_eq!(http.sent().len(), 2);
    let (model, http, _) = model_of(
        Scripted::answering(vec![
            Err(HttpError::Unanswered("timed out".into())),
            Err(HttpError::Unanswered("timed out again".into())),
            Err(HttpError::Unanswered("never read".into())),
        ]),
        tokens(LATER),
    );
    let failed = model.complete(request).unwrap_err().to_string();
    assert!(failed.contains("timed out again"), "{failed}");
    assert_eq!(
        http.sent().len(),
        2,
        "a reply may be billed: asked twice at most"
    );
}

#[test]
fn a_reply_cut_at_the_output_limit_stands_and_a_filtered_one_is_an_error() {
    let incomplete = |reason: &str| {
        streamed(&[json!({"type": "response.incomplete", "response": {
            "incomplete_details": {"reason": reason}, "usage": {"output_tokens": 9}}})])
    };
    let cut = read(&incomplete("max_output_tokens"), "m").unwrap();
    assert_eq!(cut.usage.output_tokens, 9);
    assert_eq!(cut.ending, Ending::Cut, "the agent goes on from it");
    let whole = read(&streamed(&[completed()]), "m").unwrap();
    assert_eq!(whole.ending, Ending::Whole);
    let filtered = read(&incomplete("content_filter"), "m");
    assert!(matches!(filtered, Err(ChatError::Invalid(m)) if m.contains("content filter")));
}

#[test]
fn a_request_that_never_went_out_is_sent_again() {
    let (model, http, _) = model_of(
        Scripted::answering(vec![
            Err(HttpError::Unsent("connection refused".into())),
            Ok(Reply {
                status: 200,
                headers: vec![],
                body: streamed(&[completed()]),
            }),
        ]),
        tokens(LATER),
    );
    let request = Request {
        conversation: &[],
        tools: &[],
        calling: Calling::Nothing,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    assert!(model.complete(request).is_ok());
    assert_eq!(http.sent().len(), 2);
}

#[test]
fn each_conversation_is_a_session_of_its_own_at_the_backend_all_its_requests_one() {
    // The backend sends a session's requests where its cache is by the `session-id` header (a
    // proxy may drop `session_id`) and the prompt cache's key; one shared by every session puts
    // them all on one machine, past what it takes, and the rest read the conversation afresh.
    let (model, http, _) = model(vec![(200, streamed(&[completed()])); 4], tokens(LATER));
    let start = |goal: &str| {
        vec![
            Message::System("rules".into()),
            Message::User("<context>".into()),
            Message::User(goal.into()),
        ]
    };
    let ask = |conversation: &[Message]| {
        let request = Request {
            conversation,
            tools: &[],
            calling: Calling::Any,
            deadline: None,
            effort: None,
            task: TaskId::NONE,
        };
        model.complete(request).unwrap();
    };
    let mut later = start("fix the parser");
    ask(&later);
    later.push(Message::Assistant {
        text: Some("Looking.".into()),
        calls: vec![],
        carried: vec![],
    });
    later.push(Message::User("and the tests".into()));
    ask(&later);
    ask(&start("write the docs"));
    ask(&start("You are `scout`, an agent: map the code"));
    let sent = http.sent();
    let session = |at: usize| {
        let body: Value = serde_json::from_str(&sent[at].body).unwrap();
        let header = sent[at].header("session-id").unwrap().to_string();
        assert_eq!(body["prompt_cache_key"], json!(header), "one key for both");
        assert!(
            sent[at]
                .headers
                .iter()
                .all(|(name, _)| name != "session_id")
        );
        header
    };
    assert_eq!(
        session(0),
        session(1),
        "a session's later task stays where it was"
    );
    assert_ne!(
        session(0),
        session(2),
        "another session goes where its own cache is"
    );
    assert_ne!(session(2), session(3), "and so does an agent");
}

#[test]
fn a_note_the_model_wrote_beside_its_calls_goes_back_in_its_phase() {
    // The model writes notes as it works (`commentary`) and its answer last (`final_answer`): a
    // note sent back without its phase reads, in the next request, as an answer already given.
    let note = json!({"type": "message", "id": "msg_1", "role": "assistant", "phase": "commentary",
        "content": [{"type": "output_text", "text": "Tests next."}]});
    let reply = streamed(&[
        json!({"type": "response.output_item.done", "item": note}),
        json!({"type": "response.output_item.done", "item": {"type": "function_call",
            "call_id": "c1", "name": "bash", "arguments": "{}"}}),
        completed(),
    ]);
    let completion = read(&reply, "gpt-6-sol").unwrap();
    assert_eq!(completion.text.as_deref(), Some("Tests next."));
    let conversation = vec![
        Message::User("goal".into()),
        completion.message(),
        Message::ToolResult {
            id: "c1".into(),
            content: "ok".into(),
            failed: false,
        },
    ];
    let request = Request {
        conversation: &conversation,
        tools: &[],
        calling: Calling::Any,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    let body = request_body(
        &Selection::new(entry("gpt-6-sol")),
        request,
        Texts::Functions,
    );
    let input = body["input"].as_array().unwrap();
    assert_eq!(input[1]["phase"], "commentary");
    assert!(input[1].get("id").is_none(), "nothing stored to name");
    assert_eq!(input[1]["content"][0]["text"], "Tests next.");
    assert_eq!(input[2]["type"], "function_call");
    assert_eq!(input.len(), 4, "the note once: {input:?}");
}

#[test]
fn the_models_reasoning_goes_back_with_the_next_request() {
    // With nothing stored at the backend, a turn's reasoning is lost unless it is sent back: the
    // model would think the task through again at every turn.
    let reasoning = json!({"type": "reasoning", "id": "rs_1", "summary": [],
        "encrypted_content": "sealed"});
    let reply = streamed(&[
        json!({"type": "response.output_item.done", "item": reasoning}),
        json!({"type": "response.output_item.done", "item": {"type": "reasoning", "id": "rs_2"}}),
        json!({"type": "response.output_item.done", "item": {"type": "function_call",
            "call_id": "c2", "name": "read", "arguments": "{}"}}),
        completed(),
    ]);
    let completion = read(&reply, "gpt-6-sol").unwrap();
    let conversation = vec![
        Message::User("goal".into()),
        completion.message(),
        Message::ToolResult {
            id: "c2".into(),
            content: "text".into(),
            failed: false,
        },
    ];
    let request = Request {
        conversation: &conversation,
        tools: &[],
        calling: Calling::Any,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    let body = request_body(
        &Selection::new(entry("gpt-6-sol")),
        request,
        Texts::Functions,
    );
    assert_eq!(body["include"], json!(["reasoning.encrypted_content"]));
    let input = body["input"].as_array().unwrap();
    // Only the sealed one: an item the backend did not keep cannot be named by its id alone.
    assert_eq!(input[1], reasoning);
    assert_eq!(input[2]["type"], "function_call");
    assert_eq!(input.len(), 4);

    // After `/model` switched, the reasoning another model sealed stays out: it cannot open it.
    let request = Request {
        conversation: &conversation,
        tools: &[],
        calling: Calling::Any,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    let other = request_body(
        &Selection::new(entry("gpt-6-luna")),
        request,
        Texts::Functions,
    );
    assert_eq!(other["input"][1]["type"], "function_call");

    // A model that does not reason is asked for nothing.
    let bare = Request {
        conversation: &[],
        tools: &[],
        calling: Calling::Any,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    assert_eq!(
        request_body(&Selection::new(entry("gpt-next")), bare, Texts::Functions)["include"],
        json!([])
    );
}

fn script_spec() -> ToolSpec {
    ToolSpec {
        name: "exec".into(),
        description: "Run a script".into(),
        input: Input::Text("start: /.+/".into()),
    }
}

fn ask_with(model: &ChatModel, spec: &ToolSpec) -> Completion {
    let conversation = [Message::User("goal".into())];
    let request = Request {
        conversation: &conversation,
        tools: std::slice::from_ref(spec),
        calling: Calling::Any,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    model.complete(request).unwrap()
}

#[test]
fn a_streamed_custom_call_is_the_script_as_the_model_wrote_it() {
    let script = "const r = await tools.read({path: \"a\"});\nreturn r;";
    let reply = streamed(&[
        json!({"type": "response.output_item.added", "item": {"type": "custom_tool_call",
            "call_id": "c1", "name": "exec", "input": ""}}),
        json!({"type": "response.custom_tool_call_input.delta", "call_id": "c1",
            "delta": "const r = await"}),
        json!({"type": "response.custom_tool_call_input.done", "call_id": "c1"}),
        json!({"type": "response.output_item.done", "item": {"type": "custom_tool_call",
            "call_id": "c1", "name": "exec", "input": script}}),
        completed(),
    ]);
    let (model, http, _) = model(vec![(200, reply)], tokens(LATER));
    let model = ChatModel {
        texts: Texts::Custom,
        ..model
    };
    let completion = ask_with(&model, &script_spec());
    assert_eq!(completion.calls.len(), 1);
    assert_eq!(completion.calls[0].name, "exec");
    assert_eq!(completion.calls[0].arguments, json!(script));
    let sent: Value = serde_json::from_str(&http.sent()[0].body).unwrap();
    assert_eq!(sent["tools"][0]["type"], "custom");
    assert_eq!(sent["tools"][0]["format"]["syntax"], "lark");
}

#[test]
fn a_script_called_as_a_function_is_the_text_of_its_one_argument() {
    let arguments = json!({"source": "return 1"}).to_string();
    let reply = streamed(&[
        json!({"type": "response.output_item.done", "item": {"type": "function_call",
            "call_id": "c1", "name": "exec", "arguments": arguments}}),
        completed(),
    ]);
    let (model, http, _) = model(vec![(200, reply)], tokens(LATER));
    let completion = ask_with(&model, &script_spec());
    assert_eq!(completion.calls[0].arguments, json!("return 1"));
    let sent: Value = serde_json::from_str(&http.sent()[0].body).unwrap();
    assert_eq!(sent["tools"][0]["type"], "function");
}

/// A reply that gives the routing token `state`, or none.
fn routed(state: Option<&str>) -> Reply {
    Reply {
        status: 200,
        headers: state
            .map(|state| vec![("x-codex-turn-state".to_string(), state.to_string())])
            .unwrap_or_default(),
        body: streamed(&[completed()]),
    }
}

/// The routing tokens `asked` sent, a request of `task` each, answered by `replies` in turn.
fn tokens_sent(replies: Vec<Option<&str>>, asked: &[(&[Message], u64)]) -> Vec<Option<String>> {
    let (model, http, _) = model_of(
        Scripted::answering(replies.into_iter().map(|state| Ok(routed(state))).collect()),
        tokens(LATER),
    );
    for (conversation, task) in asked {
        model
            .complete(Request {
                conversation,
                tools: &[],
                calling: Calling::Any,
                deadline: None,
                effort: None,
                task: TaskId(*task),
            })
            .unwrap();
    }
    (http.sent().iter())
        .map(|sent| sent.header("x-codex-turn-state").map(str::to_string))
        .collect()
}

#[test]
fn the_routing_token_a_reply_gives_goes_back_within_its_task_and_no_further() {
    // The backend names the machine that holds a task's cache in a header; sent back with the
    // task's next requests, they read their start from that cache instead of anew.
    let other = [
        Message::System("rules".into()),
        Message::User("another".into()),
    ];
    let mut next = conversation();
    next.push(Message::Assistant {
        text: Some("done".into()),
        calls: vec![],
        carried: vec![],
    });
    next.push(Message::User("a second goal".into()));
    let sent = tokens_sent(
        vec![Some("s1"), None, Some("s9"), None, Some("s2"), None],
        &[
            (&conversation()[..2], 1),
            (&conversation(), 1),
            // Another session on the same model (an agent) has a task of its own.
            (&other, 2),
            (&other, 2),
            (&next, 3),
            (&next, 3),
        ],
    );
    assert_eq!(
        sent,
        [
            None,
            Some("s1".into()),
            None,
            Some("s9".into()),
            None,
            Some("s2".into())
        ]
    );
}

#[test]
fn what_is_said_within_a_task_keeps_its_routing_token() {
    // Steering, an agent's answer or a note to go on reach the model as the person's words; the
    // task goes on, and so does its route.
    let mut task = conversation();
    task.insert(1, Message::User("<context>the place</context>".into()));
    let mut steered = task.clone();
    steered.push(Message::User("<steer>also the tests</steer>".into()));
    let sent = tokens_sent(vec![Some("s1"), None], &[(&task, 1), (&steered, 1)]);
    assert_eq!(sent, [None, Some("s1".into())]);
}

#[test]
fn a_task_after_a_rewind_never_takes_the_token_of_the_one_it_replaces() {
    // The same goal at the same place in the conversation, but another task.
    let goal = &conversation()[..2];
    let sent = tokens_sent(vec![Some("s1"), None], &[(goal, 1), (goal, 2)]);
    assert_eq!(sent, [None, None]);
}

#[test]
fn the_deadline_of_a_request_reaches_the_http_client() {
    let (model, http, _) = model(vec![(200, streamed(&[completed()]))], tokens(LATER));
    let conversation = conversation();
    let at = Instant::now() + Duration::from_secs(5);
    model
        .complete(Request {
            conversation: &conversation,
            tools: &[],
            calling: Calling::Nothing,
            deadline: Some(at),
            effort: None,
            task: TaskId::NONE,
        })
        .unwrap();
    assert_eq!(http.sent()[0].deadline, Some(at));
}

#[test]
fn a_refresh_is_held_to_the_deadline_of_the_request_it_comes_before() {
    // A token endpoint that hangs must not keep the request, and those waiting on it, past it.
    let refreshed = json!({"access_token": access("acct-1", LATER), "refresh_token": "r2"});
    let replies = vec![
        (200, refreshed.to_string()),
        (200, streamed(&[completed()])),
    ];
    let (model, http, _) = model(replies, tokens(1));
    let at = Instant::now() + Duration::from_secs(5);
    model
        .complete(Request {
            conversation: &[],
            tools: &[],
            calling: Calling::Nothing,
            deadline: Some(at),
            effort: None,
            task: TaskId::NONE,
        })
        .unwrap();
    let sent = http.sent();
    assert_eq!(sent[0].address, "https://auth.openai.com/oauth/token");
    assert_eq!(sent[0].deadline, Some(at));
}

#[test]
fn a_refresh_goes_to_the_token_endpoints_own_client() {
    // The backend's client waits minutes for a reply; the token endpoint's, much less.
    let refreshed = json!({"access_token": access("acct-1", LATER), "refresh_token": "r2"});
    let backend = Arc::new(Scripted::new(vec![(200, streamed(&[completed()]))]));
    let issuer = Arc::new(Scripted::new(vec![(200, refreshed.to_string())]));
    let model = ChatModel::on(
        Box::new(Shared(Arc::clone(&backend))),
        Box::new(Shared(Arc::clone(&issuer))),
        Box::new(Arc::new(Memory::default())),
        APP,
        tokens(1),
        Selection::new(entry("gpt-6-sol")),
    );
    model
        .complete(Request {
            conversation: &[],
            tools: &[],
            calling: Calling::Nothing,
            deadline: None,
            effort: None,
            task: TaskId::NONE,
        })
        .unwrap();
    let addresses = |http: &Scripted| -> Vec<String> {
        http.sent().into_iter().map(|sent| sent.address).collect()
    };
    assert_eq!(addresses(&issuer), ["https://auth.openai.com/oauth/token"]);
    assert_eq!(addresses(&backend), [format!("{BASE}/responses")]);
}

#[test]
fn a_conversations_own_level_goes_out_where_the_model_offers_it() {
    let (model, http, _) = model(vec![(200, streamed(&[completed()]))], tokens(LATER));
    let conversation = conversation();
    model
        .complete(Request {
            conversation: &conversation[..2],
            tools: &[],
            calling: Calling::Any,
            deadline: None,
            effort: Some(Effort::Low),
            task: TaskId::NONE,
        })
        .unwrap();
    let sent: Value = serde_json::from_str(&http.sent()[0].body).unwrap();
    assert_eq!(sent["reasoning"]["effort"], "low");
}
