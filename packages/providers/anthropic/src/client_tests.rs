use serde_json::json;

use super::*;
use crate::testing::scripted;

fn messages(
    answers: Vec<(u16, String)>,
) -> (
    Messages,
    std::sync::Arc<std::sync::Mutex<Vec<crate::testing::Sent>>>,
) {
    let (http, sent) = scripted(answers);
    let client = Messages::over(
        http,
        "https://api.example.com/v1/",
        "sk-test".into(),
        "Anthropic",
    );
    (client.unpaused(), sent)
}

#[test]
fn a_request_goes_to_the_messages_address_with_the_key_and_the_version() {
    let (client, sent) = messages(vec![(200, json!({"content": []}).to_string())]);
    client.post(&json!({"model": "m"}), None, None).unwrap();
    let sent = sent.lock().unwrap();
    assert_eq!(sent[0].address, "https://api.example.com/v1/messages");
    let header = |name: &str| {
        (sent[0].headers.iter())
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(header("x-api-key"), Some("sk-test"));
    assert_eq!(header("anthropic-version"), Some(VERSION));
    assert_eq!(
        header("authorization"),
        None,
        "a key is never sent as a bearer token"
    );
}

#[test]
fn an_overloaded_host_is_asked_again_and_a_refused_key_is_not() {
    let overloaded =
        json!({"type": "error", "error": {"type": "overloaded_error", "message": "busy"}});
    let (client, sent) = messages(vec![
        (529, overloaded.to_string()),
        (200, json!({"content": [], "model": "m"}).to_string()),
    ]);
    let (message, _) = client.post(&json!({}), None, None).unwrap();
    assert_eq!(message["model"], "m");
    assert_eq!(sent.lock().unwrap().len(), 2);
    let refused = json!({"type": "error", "error": {"type": "authentication_error", "message": "invalid x-api-key"}});
    let (refusing, sent) = messages(vec![(401, refused.to_string())]);
    assert_eq!(
        refusing.post(&json!({}), None, None),
        Err(ApiError::Auth("invalid x-api-key".into()))
    );
    assert_eq!(sent.lock().unwrap().len(), 1);
}

#[test]
fn a_stream_that_failed_after_its_first_words_is_not_paid_for_twice() {
    let body = crate::testing::stream(&[
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": "Hal"}}),
        json!({"type": "error", "error": {"type": "overloaded_error", "message": "busy"}}),
    ]);
    let (client, sent) = messages(vec![(200, body)]);
    assert_eq!(
        client.post(&json!({}), None, None),
        Err(ApiError::Http(529, "busy".into()))
    );
    assert_eq!(sent.lock().unwrap().len(), 1);
}

#[test]
fn a_rate_limit_is_waited_out_as_long_as_the_api_asks() {
    use provider_kit::http::Reply;
    let limited = Reply {
        status: 429,
        headers: vec![("retry-after".into(), "0.05".into())],
        body: json!({"type": "error", "error": {"type": "rate_limit_error", "message": "slow"}})
            .to_string(),
    };
    let fine = Reply {
        status: 200,
        headers: vec![],
        body: json!({"content": []}).to_string(),
    };
    let (http, sent) = crate::testing::replying(vec![limited, fine]);
    let client = Messages::over(http, "https://api.example.com/v1/", "k".into(), "Anthropic");
    let started = Instant::now();
    client.unpaused().post(&json!({}), None, None).unwrap();
    assert_eq!(sent.lock().unwrap().len(), 2);
    assert!(started.elapsed() >= Duration::from_millis(50));
}
