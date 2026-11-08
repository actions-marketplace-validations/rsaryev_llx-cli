use serde_json::json;

use super::*;
use crate::testing::stream;

#[test]
fn a_stream_is_put_back_together_as_the_message_sent_whole() {
    let body = stream(&[
        json!({"type": "message_start", "message": {"model": "claude-x", "usage": {
            "input_tokens": 12, "cache_read_input_tokens": 9000,
            "cache_creation_input_tokens": 300, "output_tokens": 1}}}),
        json!({"type": "content_block_start", "index": 0,
            "content_block": {"type": "thinking", "thinking": ""}}),
        json!({"type": "content_block_delta", "index": 0,
            "delta": {"type": "thinking_delta", "thinking": "Read it "}}),
        json!({"type": "content_block_delta", "index": 0,
            "delta": {"type": "signature_delta", "signature": "sig"}}),
        json!({"type": "content_block_stop", "index": 0}),
        json!({"type": "ping"}),
        json!({"type": "content_block_start", "index": 1,
            "content_block": {"type": "text", "text": ""}}),
        json!({"type": "content_block_delta", "index": 1,
            "delta": {"type": "text_delta", "text": "Checking."}}),
        json!({"type": "content_block_start", "index": 2,
            "content_block": {"type": "tool_use", "id": "t1", "name": "bash", "input": {}}}),
        json!({"type": "content_block_delta", "index": 2,
            "delta": {"type": "input_json_delta", "partial_json": "{\"command\": "}}),
        json!({"type": "content_block_delta", "index": 2,
            "delta": {"type": "input_json_delta", "partial_json": "\"ls\"}"}}),
        json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"},
            "usage": {"output_tokens": 40}}),
        json!({"type": "message_stop"}),
    ]);
    let message = assembled(&body).unwrap();
    assert_eq!(message["model"], "claude-x");
    assert_eq!(message["stop_reason"], "tool_use");
    assert_eq!(
        message["content"][0],
        json!({"type": "thinking", "thinking": "Read it ", "signature": "sig"})
    );
    assert_eq!(message["content"][1]["text"], "Checking.");
    assert_eq!(message["content"][2]["input"], json!({"command": "ls"}));
    assert_eq!(message["usage"]["output_tokens"], 40);
    assert_eq!(message["usage"]["cache_read_input_tokens"], 9000);
}

#[test]
fn a_call_the_output_limit_cut_off_keeps_the_text_it_wrote() {
    let body = stream(&[
        json!({"type": "content_block_start", "index": 0,
            "content_block": {"type": "tool_use", "id": "t1", "name": "apply_patch", "input": {}}}),
        json!({"type": "content_block_delta", "index": 0,
            "delta": {"type": "input_json_delta", "partial_json": "{\"source\": \"*** Begin"}}),
        json!({"type": "message_delta", "delta": {"stop_reason": "max_tokens"}}),
    ]);
    let message = assembled(&body).unwrap();
    assert_eq!(message["content"][0]["input"], "{\"source\": \"*** Begin");
    assert_eq!(message["stop_reason"], "max_tokens");
}

#[test]
fn an_error_midway_is_the_answer_and_says_whether_words_came_before_it() {
    let error = json!({"type": "error", "error": {"type": "overloaded_error", "message": "busy"}});
    let early = assembled(&stream(std::slice::from_ref(&error))).unwrap();
    assert_eq!(
        early,
        json!({"error": {"type": "overloaded_error", "message": "busy"}})
    );
    let late = assembled(&stream(&[
        json!({"type": "content_block_start", "index": 0,
            "content_block": {"type": "text", "text": "Hal"}}),
        error,
    ]))
    .unwrap();
    assert_eq!(late["written"], true);
    assert_eq!(assembled("{\"not\": \"a stream\"}"), None);
}
