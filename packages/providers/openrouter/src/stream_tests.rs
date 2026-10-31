use std::fmt::Write as _;

use serde_json::json;

use super::*;

/// `events` as a host streams them: each a `data:` line, then `[DONE]`.
fn streamed(events: &[Value]) -> String {
    let mut body = String::from(": OPENROUTER PROCESSING\n\n");
    for event in events {
        let _ = write!(body, "data: {event}\n\n");
    }
    body.push_str("data: [DONE]\n\n");
    body
}

fn delta(delta: &Value) -> Value {
    json!({"model": "lab/model", "provider": "Lab", "choices": [{"index": 0, "delta": delta}]})
}

#[test]
fn a_streamed_reply_is_put_back_together_as_the_one_sent_whole() {
    let body = streamed(&[
        delta(&json!({"role": "assistant", "reasoning": "Count ",
            "reasoning_details": [{"type": "reasoning.text", "text": "Count ", "format": "f",
                "index": 0}]})),
        delta(&json!({"reasoning": "them.",
            "reasoning_details": [{"type": "reasoning.text", "text": "them.", "format": "f",
                "index": 0}]})),
        delta(
            &json!({"reasoning_details": [{"type": "reasoning.text", "signature": "sig",
            "format": "f", "index": 0}]}),
        ),
        delta(&json!({"content": "Reading "})),
        delta(
            &json!({"content": "it.", "tool_calls": [{"index": 0, "id": "t1", "type": "function",
            "function": {"name": "bash", "arguments": ""}}]}),
        ),
        delta(&json!({"tool_calls": [{"index": 0, "function": {"arguments": "{\"command\":"}}]})),
        delta(&json!({"tool_calls": [{"index": 0, "function": {"arguments": "\"ls\"}"}}]})),
        json!({"model": "lab/model", "choices": [{"index": 0, "delta": {"content": ""},
            "finish_reason": "tool_calls"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 4, "cost": 0.5}}),
    ]);
    let whole = assembled(&body).unwrap();
    assert_eq!(whole["provider"], "Lab");
    assert_eq!(whole["usage"]["cost"], 0.5);
    let choice = &whole["choices"][0];
    assert_eq!(choice["finish_reason"], "tool_calls");
    let message = &choice["message"];
    assert_eq!(message["content"], "Reading it.");
    assert_eq!(message["reasoning"], "Count them.");
    assert_eq!(
        message["reasoning_details"],
        json!([{"type": "reasoning.text", "text": "Count them.", "format": "f", "index": 0,
            "signature": "sig"}]),
        "the parts of one detail are one detail again, its signature kept"
    );
    assert_eq!(
        message["tool_calls"],
        json!([{"id": "t1", "type": "function",
            "function": {"name": "bash", "arguments": "{\"command\":\"ls\"}"}}])
    );
}

#[test]
fn a_reply_cut_at_the_limit_keeps_what_its_last_call_wrote() {
    // Sent whole, a call the limit cut off comes back with `{}`: what it wrote is lost.
    let body = streamed(&[
        delta(
            &json!({"tool_calls": [{"index": 0, "id": "t1", "type": "function",
            "function": {"name": "apply_patch", "arguments": "{\"source\": \"*** Begin"}}]}),
        ),
        json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "length"}]}),
    ]);
    let whole = assembled(&body).unwrap();
    assert_eq!(whole["choices"][0]["finish_reason"], "length");
    assert_eq!(
        whole["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
        "{\"source\": \"*** Begin"
    );
}

#[test]
fn an_error_midway_through_a_stream_is_the_answer() {
    let body = streamed(&[
        delta(&json!({"content": "Half"})),
        json!({"error": {"code": 502, "message": "upstream died"},
            "choices": [{"index": 0, "delta": {}, "finish_reason": "error"}]}),
    ]);
    assert_eq!(
        assembled(&body),
        Some(json!({"error": {"code": 502, "message": "upstream died"}}))
    );
}

#[test]
fn a_body_that_is_not_a_stream_is_none() {
    assert_eq!(assembled("not a stream"), None);
    assert_eq!(assembled(""), None);
}
