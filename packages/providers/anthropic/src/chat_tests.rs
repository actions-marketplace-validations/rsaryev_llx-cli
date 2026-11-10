use contracts::catalog::Effort;
use contracts::chat::{Calling, Chat, ChatError, Completion, Ending, Message, Request, TaskId};
use contracts::tool::{Input, ToolSpec};
use std::sync::{Arc, Mutex};

use serde_json::json;

use super::*;
use crate::testing::{scripted, selection, stream};

fn model(answers: Vec<(u16, String)>) -> (ChatModel, Arc<Mutex<Vec<crate::testing::Sent>>>) {
    let (http, sent) = scripted(answers);
    let client =
        Messages::over(http, "https://api.example.com/v1", "sk".into(), "Anthropic").unpaused();
    let model = ChatModel::new(
        client,
        selection("claude-x", vec![Effort::Low, Effort::High], Effort::High),
        Thinking::Adaptive,
    );
    (model, sent)
}

fn patch_tool() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "apply_patch".into(),
        description: "Change files.".into(),
        input: Input::Text("start: patch".into()),
    }]
}

fn ask<'a>(
    model: &ChatModel,
    conversation: &'a [Message],
    tools: &'a [ToolSpec],
) -> Result<Completion, ChatError> {
    model.complete(Request {
        conversation,
        tools,
        calling: Calling::Any,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    })
}

#[test]
fn a_turn_comes_back_with_its_calls_its_signed_thinking_and_what_the_cache_read_and_wrote() {
    let reply = stream(&[
        json!({"type": "message_start", "message": {"model": "claude-x", "usage": {
            "input_tokens": 50, "cache_read_input_tokens": 140_000,
            "cache_creation_input_tokens": 2_000, "output_tokens": 1}}}),
        json!({"type": "content_block_start", "index": 0,
            "content_block": {"type": "thinking", "thinking": "", "signature": "sig"}}),
        json!({"type": "content_block_start", "index": 1,
            "content_block": {"type": "tool_use", "id": "t1", "name": "apply_patch", "input": {}}}),
        json!({"type": "content_block_delta", "index": 1, "delta": {"type": "input_json_delta",
            "partial_json": "{\"source\": \"*** Begin Patch\\n*** End Patch\"}"}}),
        json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 90}}),
    ]);
    let (model, _) = model(vec![(200, reply)]);
    let completion = ask(&model, &[Message::User("go".into())], &patch_tool()).unwrap();
    assert_eq!(
        completion.calls[0].arguments,
        json!("*** Begin Patch\n*** End Patch")
    );
    assert_eq!(completion.text, None);
    assert_eq!(completion.thought, None, "omitted thinking is not shown");
    assert_eq!(completion.carried[0]["anthropic"][0]["signature"], "sig");
    let usage = &completion.usage;
    assert_eq!(
        (
            usage.input_tokens,
            usage.cached_tokens,
            usage.written_tokens,
            usage.output_tokens
        ),
        (142_050, 140_000, 2_000, 90)
    );
    // The next turn sends the thinking back with the call it came with.
    let next = [
        Message::User("go".into()),
        completion.message(),
        Message::ToolResult {
            id: "t1".into(),
            content: "done".into(),
            failed: false,
        },
    ];
    let (again, sent) = self::model(vec![(
        200,
        json!({"content": [{"type": "text", "text": "Done."}], "stop_reason": "end_turn"})
            .to_string(),
    )]);
    assert_eq!(
        ask(&again, &next, &patch_tool()).unwrap().text.as_deref(),
        Some("Done.")
    );
    let body = &sent.lock().unwrap()[0].body;
    assert_eq!(body["messages"][1]["content"][0]["signature"], "sig");
    assert_eq!(body["messages"][1]["content"][1]["type"], "tool_use");
}

#[test]
fn a_patch_the_output_limit_cut_off_keeps_what_it_wrote() {
    let reply = stream(&[
        json!({"type": "content_block_start", "index": 0,
            "content_block": {"type": "tool_use", "id": "t1", "name": "apply_patch", "input": {}}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "input_json_delta",
            "partial_json": "{\"source\": \"*** Begin Patch\\n*** Add File: a\\n+one"}}),
        json!({"type": "message_delta", "delta": {"stop_reason": "max_tokens"}}),
    ]);
    let (model, _) = model(vec![(200, reply)]);
    let completion = ask(&model, &[Message::User("go".into())], &patch_tool()).unwrap();
    assert_eq!(completion.ending, Ending::Cut);
    assert_eq!(
        completion.calls[0].arguments,
        json!("*** Begin Patch\n*** Add File: a\n+one")
    );
}

#[test]
fn thinking_bound_to_a_history_that_changed_is_dropped_and_the_turn_asked_once_more() {
    let conversation = [
        Message::User("go".into()),
        Message::Assistant {
            text: Some("ok".into()),
            calls: vec![],
            carried: vec![json!({"model": "claude-x", "anthropic": [
                {"type": "thinking", "thinking": "", "signature": "old"},
                {"type": "text", "text": "ok"}]})],
        },
        Message::User("more".into()),
    ];
    let refused = json!({"type": "error", "error": {"type": "invalid_request_error",
        "message": "messages.1.content.0: Invalid `signature` in `thinking` block."}});
    let (model, sent) = model(vec![
        (400, refused.to_string()),
        (
            200,
            json!({"content": [{"type": "text", "text": "fine"}], "stop_reason": "end_turn"})
                .to_string(),
        ),
        (
            200,
            json!({"content": [{"type": "text", "text": "again"}], "stop_reason": "end_turn"})
                .to_string(),
        ),
    ]);
    assert_eq!(
        ask(&model, &conversation, &[]).unwrap().text.as_deref(),
        Some("fine")
    );
    assert!(sent.lock().unwrap()[0].body.to_string().contains("old"));
    assert!(!sent.lock().unwrap()[1].body.to_string().contains("old"));
    // The next request leaves the refused block out at once: no refusal at every turn after.
    assert_eq!(
        ask(&model, &conversation, &[]).unwrap().text.as_deref(),
        Some("again")
    );
    assert_eq!(sent.lock().unwrap().len(), 3);
    assert!(!sent.lock().unwrap()[2].body.to_string().contains("old"));
}

#[test]
fn a_budgeted_model_thinks_between_calls_and_is_asked_once_without_thinking_when_refused() {
    let (http, sent) = scripted(vec![
        (
            400,
            json!({"type": "error", "error": {"type": "invalid_request_error",
            "message": "Expected `thinking` or `redacted_thinking`, but found `tool_use`."}})
            .to_string(),
        ),
        (
            200,
            json!({"content": [{"type": "text", "text": "ok"}], "stop_reason": "end_turn"})
                .to_string(),
        ),
    ]);
    let client =
        Messages::over(http, "https://api.example.com/v1", "sk".into(), "Anthropic").unpaused();
    let older = ChatModel::new(
        client,
        selection("claude-old", vec![Effort::Low, Effort::High], Effort::High),
        Thinking::Budget,
    );
    assert!(ask(&older, &[Message::User("go".into())], &[]).is_ok());
    let sent = sent.lock().unwrap();
    let beta = (sent[0].headers.iter()).find(|(name, _)| name == "anthropic-beta");
    assert_eq!(
        beta.map(|(_, v)| v.as_str()),
        Some("interleaved-thinking-2025-05-14")
    );
    assert_eq!(sent[0].body["thinking"]["type"], "enabled");
    assert!(sent[1].body.get("thinking").is_none());
}

#[test]
fn a_declined_request_is_an_error_that_says_why() {
    // Midway, with words written before it: still no answer to keep.
    let declined = json!({"content": [{"type": "text", "text": "Sure, here"}],
        "stop_reason": "refusal", "stop_details": {"type": "refusal", "explanation": "not this"}});
    let (declining, _) = model(vec![(200, declined.to_string())]);
    let error = ask(&declining, &[Message::User("go".into())], &[]).unwrap_err();
    assert_eq!(
        error,
        ChatError::Invalid("the model declined to answer: not this".into())
    );
    let (unfinished, _) = model(vec![(
        200,
        json!({"content": [{"type": "text", "text": "Hal"}]}).to_string(),
    )]);
    assert!(ask(&unfinished, &[Message::User("go".into())], &[]).is_err());
}

#[test]
fn an_old_turns_thinking_may_go_its_words_and_calls_stay_in_order() {
    let mut kept = json!({"model": "claude", "anthropic": [
        {"type": "thinking", "thinking": "Plan", "signature": "s1"},
        {"type": "text", "text": "Reading it."},
        {"type": "redacted_thinking", "data": "sealed"},
        {"type": "tool_use", "id": "t1", "name": "bash", "input": {}},
    ]});
    assert!(shed(&mut kept));
    assert_eq!(
        kept,
        json!({"model": "claude", "anthropic": [
            {"type": "text", "text": "Reading it."},
            {"type": "tool_use", "id": "t1", "name": "bash", "input": {}},
        ]})
    );
    assert!(!shed(&mut kept), "nothing more to go");
    let mut other =
        json!({"model": "gpt", "item": {"type": "reasoning", "encrypted_content": "e"}});
    let before = other.clone();
    assert!(!shed(&mut other));
    assert_eq!(other, before);
}
