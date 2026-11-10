use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use contracts::catalog::{Effort, ModelEntry, Selection};
use contracts::chat::{Calling, Chat, Message, Request, TaskId};
use provider_kit::http::Reply;
use serde_json::json;

use super::*;

const APP: App = App {
    name: "llx",
    slug: "llx",
    version: "0",
    site: "https://llx.sh",
};

/// An `Http` that gives each request the next of its answers (a reply, or why none came), and
/// keeps every request body.
struct Scripted {
    answers: Mutex<Vec<Result<Reply, HttpError>>>,
    sent: Arc<Mutex<Vec<Value>>>,
}

impl Http for Scripted {
    #[expect(
        clippy::unwrap_in_result,
        reason = "a test's fake: a poisoned lock fails the test"
    )]
    fn post(
        &self,
        _: &str,
        _: &[(&str, &str)],
        body: &str,
        _: Option<Instant>,
    ) -> Result<Reply, HttpError> {
        let body = serde_json::from_str(body).unwrap_or(Value::Null);
        self.sent.lock().unwrap().push(body);
        let next = self.answers.lock().unwrap().pop();
        next.unwrap_or_else(|| Err(HttpError::Unanswered("nothing scripted".into())))
    }
}

/// `client` on a scripted `Http` answering `answers` in order; the bodies it was sent.
fn scripted(
    client: OpenRouter,
    answers: Vec<Result<(u16, &str), HttpError>>,
) -> (OpenRouter, Arc<Mutex<Vec<Value>>>) {
    let answers = (answers.into_iter())
        .map(|answer| {
            answer.map(|(status, body)| Reply {
                status,
                headers: vec![],
                body: body.into(),
            })
        })
        .collect();
    replying(client, answers)
}

/// The same, with whole replies: their headers too.
fn replying(
    client: OpenRouter,
    answers: Vec<Result<Reply, HttpError>>,
) -> (OpenRouter, Arc<Mutex<Vec<Value>>>) {
    let answers = answers.into_iter().rev().collect();
    let sent = Arc::new(Mutex::new(vec![]));
    let http = Scripted {
        answers: Mutex::new(answers),
        sent: Arc::clone(&sent),
    };
    let client = OpenRouter {
        http: Box::new(http),
        pause: Duration::ZERO,
        ..client
    };
    (client, sent)
}

/// An `Http` that keeps the headers of each request and answers it `{}`.
struct Heard(Arc<Mutex<Vec<(String, String)>>>);

impl Http for Heard {
    #[expect(
        clippy::unwrap_in_result,
        reason = "a test's fake: a poisoned lock fails the test"
    )]
    fn post(
        &self,
        _: &str,
        headers: &[(&str, &str)],
        _: &str,
        _: Option<Instant>,
    ) -> Result<Reply, HttpError> {
        let mut heard = self.0.lock().unwrap();
        heard.extend((headers.iter()).map(|(name, value)| (name.to_string(), value.to_string())));
        Ok(Reply {
            status: 200,
            headers: vec![],
            body: "{}".into(),
        })
    }
}

#[test]
fn openrouter_is_told_which_app_calls_it_and_another_host_is_not() {
    // OpenRouter's attribution: its title makes an app's page only beside the referer.
    let heard = |client: OpenRouter| {
        let heard = Arc::new(Mutex::new(vec![]));
        let client = OpenRouter {
            http: Box::new(Heard(Arc::clone(&heard))),
            ..client
        };
        client.post("/x", &json!({}), None).unwrap();
        heard.lock().unwrap().clone()
    };
    let openrouter = heard(OpenRouter::standing_in("http://lab.test", "k".into(), APP));
    for header in [
        ("HTTP-Referer", "https://llx.sh"),
        ("X-OpenRouter-Title", "llx"),
    ] {
        let header = (header.0.to_string(), header.1.to_string());
        assert!(openrouter.contains(&header), "{openrouter:?}");
    }
    let other = heard(lab());
    assert!(
        !(other.iter()).any(
            |(name, _)| ["HTTP-Referer", "X-OpenRouter-Title", "X-Title"].contains(&name.as_str())
        ),
        "{other:?}"
    );
}

fn lab() -> OpenRouter {
    OpenRouter::at("http://lab.test", "k".into(), APP, "Lab")
}

#[test]
fn an_overloaded_answer_is_retried_and_a_refused_key_is_not() {
    let (client, sent) = scripted(lab(), vec![Ok((503, "{}")), Ok((200, r#"{"ok":1}"#))]);
    assert_eq!(
        client.post("/x", &json!({}), None).unwrap().0,
        json!({"ok": 1})
    );
    assert_eq!(sent.lock().unwrap().len(), 2);

    let refused = r#"{"error":{"message":"User not found."}}"#;
    let (client, sent) = scripted(lab(), vec![Ok((401, refused))]);
    let error = client.post("/x", &json!({}), None).unwrap_err();
    assert_eq!(error, ApiError::Auth("User not found.".into()));
    assert_eq!(sent.lock().unwrap().len(), 1);
    assert_eq!(
        format!("Lab {error}"),
        "Lab refused the key: User not found.; type /login, or check the key's variable"
    );
    // A permission or a moderation flag, not a key refused: no /login to send the person to.
    let flagged = r#"{"error":{"message":"Flagged by moderation."}}"#;
    let (client, _) = scripted(lab(), vec![Ok((403, flagged))]);
    let error = client.post("/x", &json!({}), None).unwrap_err();
    assert_eq!(error, ApiError::Http(403, "Flagged by moderation.".into()));
}

#[test]
fn a_request_sent_and_not_answered_is_not_sent_again_and_one_not_sent_is() {
    // A reply being written is billed: sending it again pays for it twice.
    let unanswered = HttpError::Unanswered("timed out".into());
    let (client, sent) = scripted(lab(), vec![Err(unanswered)]);
    let error = client.post("/x", &json!({}), None).unwrap_err();
    assert_eq!(error, ApiError::Network("timed out".into()));
    assert_eq!(sent.lock().unwrap().len(), 1, "sent once");
    let unsent = HttpError::Unsent("connection refused".into());
    let (client, sent) = scripted(lab(), vec![Err(unsent), Ok((200, "{}"))]);
    assert_eq!(client.post("/x", &json!({}), None).unwrap().0, json!({}));
    assert_eq!(sent.lock().unwrap().len(), 2, "sent again");
}

#[test]
fn a_decision_not_answered_is_asked_once_more() {
    // A decision is short and cheap: one that hangs is better asked again than waited on.
    let deciding = OpenRouter::new("k".into(), APP).deciding();
    assert_eq!(deciding.wire(), Wire::OpenRouter);
    let unanswered = || Err(HttpError::Unanswered("timed out".into()));
    let (client, sent) = scripted(deciding, vec![unanswered(), unanswered(), unanswered()]);
    let error = client.post("/x", &json!({}), None).unwrap_err();
    assert!(matches!(error, ApiError::Network(_)), "{error:?}");
    assert_eq!(sent.lock().unwrap().len(), 2, "sent twice, not more");
}

#[test]
fn openrouter_at_another_address_is_still_sent_openrouters_fields() {
    // A server standing in for OpenRouter at a local address must see its wire.
    let reply = r#"{"choices":[{"message":{"content":"hi"}}]}"#;
    let local = OpenRouter::standing_in("http://127.0.0.1:1", "k".into(), APP);
    assert_eq!(local.wire(), Wire::OpenRouter);
    assert_eq!(lab().wire(), Wire::Plain);
    let openai = OpenRouter::at("https://api.openai.com/v1/", "k".into(), APP, "OpenAI");
    assert_eq!(openai.wire(), Wire::OpenAi);
    let (client, sent) = scripted(local, vec![Ok((200, reply))]);
    let model = ModelEntry {
        id: "lab/model".into(),
        name: "Model".into(),
        efforts: vec![Effort::Low, Effort::High],
        default: Effort::High,
        context: 1000,
        output: 0,
        marks: false,
    };
    let chat = crate::chat::ChatModel {
        client,
        selection: Selection::new(model),
        switch: crate::level::Switch::Named,
    };
    let conversation = [Message::User("goal".into())];
    let request = Request {
        conversation: &conversation,
        tools: &[],
        calling: Calling::Any,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    assert_eq!(chat.complete(request).unwrap().text.as_deref(), Some("hi"));
    let sent = sent.lock().unwrap()[0].clone();
    assert_eq!(sent["reasoning"], json!({"effort": "high"}));
    assert_eq!(sent["usage"], json!({"include": true}));
}

/// A server whose one answer promises 100 bytes, sends a few, and closes the connection.
fn cut_short() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::Builder::new().name("llx-test-cut".into());
    server
        .spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            let mut length = 0;
            while reader.read_line(&mut line).unwrap_or(0) > 2 {
                let lower = line.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
                line.clear();
            }
            // The request read whole: a close with bytes unread resets the connection, which the
            // client may meet before the cut answer (EINVAL on macOS), not the cut it tests.
            let _ = reader.read_exact(&mut vec![0; length]);
            let mut stream = stream;
            let head = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 100";
            let _ = write!(stream, "{head}\r\n\r\n{{\"choices\"");
        })
        .unwrap();
    base
}

#[test]
fn an_answer_cut_short_is_said_so_not_taken_for_one_that_is_not_json() {
    let client = OpenRouter::at(&cut_short(), "k".into(), APP, "Lab");
    let error = client.post("/x", &json!({}), None).unwrap_err();
    let ApiError::Network(why) = &error else {
        panic!("{error:?}");
    };
    assert!(why.contains("answer"), "{why}");
}

#[test]
fn a_streamed_answer_reads_as_the_one_sent_whole_and_an_error_in_it_as_an_error() {
    let streamed = "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\ndata: [DONE]\n\n";
    let (client, _) = scripted(lab(), vec![Ok((200, streamed))]);
    let (value, _) = client.post("/x", &json!({}), None).unwrap();
    assert_eq!(value["choices"][0]["message"]["content"], "hi");
    let failed = "data: {\"error\":{\"code\":400,\"message\":\"bad\"}}\n\n";
    let (client, _) = scripted(lab(), vec![Ok((200, failed))]);
    let error = client.post("/x", &json!({}), None).unwrap_err();
    assert_eq!(error, ApiError::Http(400, "bad".into()));
}

#[test]
fn a_conversations_own_level_goes_out_where_the_model_offers_it() {
    let reply = r#"{"choices":[{"message":{"content":"hi"}}]}"#;
    let local = OpenRouter::standing_in("http://127.0.0.1:1", "k".into(), APP);
    let (client, sent) = scripted(local, vec![Ok((200, reply)), Ok((200, reply))]);
    let model = ModelEntry {
        id: "lab/model".into(),
        name: "Model".into(),
        efforts: vec![Effort::Low, Effort::High],
        default: Effort::High,
        context: 1000,
        output: 0,
        marks: false,
    };
    let chat = crate::chat::ChatModel {
        client,
        selection: Selection::new(model),
        switch: crate::level::Switch::Named,
    };
    let conversation = [Message::User("goal".into())];
    for effort in [Effort::Low, Effort::Medium] {
        let request = Request {
            conversation: &conversation,
            tools: &[],
            calling: Calling::Any,
            deadline: None,
            effort: Some(effort),
            task: TaskId::NONE,
        };
        chat.complete(request).unwrap();
    }
    let sent = sent.lock().unwrap();
    assert_eq!(sent[0]["reasoning"], json!({"effort": "low"}));
    assert_eq!(
        sent[1]["reasoning"],
        json!({"effort": "high"}),
        "not offered: the chosen one"
    );
}

#[test]
fn a_reply_with_a_null_error_is_the_completion_not_a_failure() {
    // A compatible host may say `"error": null` beside a completion that went well.
    let whole = r#"{"choices":[{"message":{"content":"Hi"}}],"error":null}"#;
    let streamed = "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}],\"error\":null}\n\n\
        data: [DONE]\n\n";
    for body in [whole, streamed] {
        let (client, sent) = scripted(lab(), vec![Ok((200, body))]);
        let (completion, _) = client.post("/x", &json!({}), None).unwrap();
        assert_eq!(
            completion.pointer("/choices/0/message/content"),
            Some(&json!("Hi")),
            "{body}"
        );
        assert_eq!(sent.lock().unwrap().len(), 1, "sent once");
    }
}

#[test]
fn a_stream_that_failed_after_its_first_words_is_not_sent_again_and_one_before_them_is() {
    // Its words were written, and billed: sending it again would pay for them twice.
    let midway = "data: {\"choices\":[{\"delta\":{\"content\":\"Half\"}}]}\n\n\
        data: {\"error\":{\"code\":502,\"message\":\"upstream died\"}}\n\n";
    let (client, sent) = scripted(lab(), vec![Ok((200, midway)), Ok((200, "{}"))]);
    let error = client.post("/x", &json!({}), None).unwrap_err();
    assert_eq!(error, ApiError::Http(502, "upstream died".into()));
    assert_eq!(sent.lock().unwrap().len(), 1, "sent once");
    let before = "data: {\"error\":{\"code\":502,\"message\":\"upstream died\"}}\n\n";
    let (client, sent) = scripted(lab(), vec![Ok((200, before)), Ok((200, "{}"))]);
    assert_eq!(client.post("/x", &json!({}), None).unwrap().0, json!({}));
    assert_eq!(sent.lock().unwrap().len(), 2, "sent again");
}

#[test]
fn a_spending_limit_of_a_moment_is_waited_out_and_one_of_the_account_is_not() {
    // OpenRouter's in-flight budget answers 402 with a Retry-After; credits run out answer 402
    // with none.
    let limited = Reply {
        status: 402,
        headers: vec![("retry-after".into(), "0".into())],
        body: r#"{"error":{"message":"in flight"}}"#.into(),
    };
    let fine = Reply {
        status: 200,
        headers: vec![],
        body: r#"{"ok":1}"#.into(),
    };
    let (client, sent) = replying(lab(), vec![Ok(limited), Ok(fine)]);
    assert!(client.post("/x", &json!({}), None).is_ok());
    assert_eq!(sent.lock().unwrap().len(), 2);
    let (client, sent) = scripted(lab(), vec![Ok((402, r#"{"error":{"message":"credits"}}"#))]);
    assert!(client.post("/x", &json!({}), None).is_err());
    assert_eq!(sent.lock().unwrap().len(), 1);
}
