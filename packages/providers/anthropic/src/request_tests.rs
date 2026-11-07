use std::collections::HashSet;

use contracts::catalog::Effort;
use contracts::chat::{Calling, Message, Request, TaskId, ToolCall};
use contracts::tool::{Image, Input, ToolSpec};
use serde_json::{Value, json};

use super::*;
use crate::testing::selection;

fn tools() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "bash".into(),
            description: "Run a command.".into(),
            input: Input::Json(
                json!({"type": "object", "properties": {"command": {"type": "string"}}}),
            ),
        },
        ToolSpec {
            name: "apply_patch".into(),
            description: "Change files.".into(),
            input: Input::Text("start: patch".into()),
        },
    ]
}

fn call(id: &str, name: &str, arguments: Value) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: name.into(),
        arguments,
    }
}

/// A session two turns in: the system prompt, the context, the goal, a turn of two calls (one a
/// patch) with its thinking, their results and a picture, another turn, its result.
fn conversation() -> Vec<Message> {
    let thought = json!({"type": "thinking", "thinking": "", "signature": "sig-1"});
    vec![
        Message::System("rules".into()),
        Message::User("<context>".into()),
        Message::User("fix it".into()),
        Message::Assistant {
            text: Some("Looking.".into()),
            calls: vec![
                call("c1", "bash", json!({"command": "ls"})),
                call("c2", "apply_patch", json!("*** Begin Patch\n*** End Patch")),
            ],
            carried: vec![
                json!({"model": "claude-x", BLOCKS: [thought, {"type": "text", "text": "Looking."},
                    {"type": "tool_use", "id": "c1"}, {"type": "tool_use", "id": "c2"}]}),
                json!({"model": "gpt", "fields": {"reasoning": "theirs"}}),
            ],
        },
        Message::ToolResult {
            id: "c1".into(),
            content: "a.rs".into(),
            failed: false,
        },
        Message::ToolResult {
            id: "c2".into(),
            content: "no such file".into(),
            failed: true,
        },
        Message::Image {
            caption: "shot.png".into(),
            image: Image {
                media_type: "image/png".into(),
                data: "AAAA".into(),
                width: 1,
                height: 1,
            },
        },
        Message::Assistant {
            text: None,
            calls: vec![call("c3", "bash", json!({"command": "cat a.rs"}))],
            carried: vec![],
        },
        Message::ToolResult {
            id: "c3".into(),
            content: String::new(),
            failed: false,
        },
    ]
}

fn sent(messages: &[Message], tools: &[ToolSpec], effort: Effort, forgotten: &[&str]) -> Value {
    let selection = selection("claude-x", vec![Effort::Low, Effort::High], effort);
    let request = Request {
        conversation: messages,
        tools,
        calling: Calling::Any,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    let forgotten: HashSet<String> = forgotten.iter().map(|s| (*s).to_string()).collect();
    body(&selection, Some(Thinking::Adaptive), &request, &forgotten)
}

#[test]
fn the_conversation_goes_as_alternating_turns_with_each_calls_results_right_after_it() {
    let body = sent(&conversation(), &tools(), Effort::High, &[]);
    assert_eq!(body["system"][0]["text"], "rules");
    let turns = body["messages"].as_array().unwrap();
    let roles: Vec<&str> = turns.iter().map(|t| t["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["user", "assistant", "user", "assistant", "user"]);
    // The context and the goal: one turn of two blocks.
    assert_eq!(turns[0]["content"][1]["text"], "fix it");
    // The thinking first, signed as it came; another provider's reasoning left out.
    let said = turns[1]["content"].as_array().unwrap();
    assert_eq!(said[0]["signature"], "sig-1");
    assert_eq!(said[1]["text"], "Looking.");
    assert_eq!(said[2]["input"], json!({"command": "ls"}));
    // The patch goes back in the one argument it was offered with.
    assert_eq!(said[3]["input"]["source"], "*** Begin Patch\n*** End Patch");
    assert_eq!(said.len(), 4, "{said:?}");
    // Both results, then the picture, in the one user turn after the calls.
    let results = turns[2]["content"].as_array().unwrap();
    assert_eq!(results[0]["tool_use_id"], "c1");
    assert_eq!(results[1]["is_error"], true);
    assert_eq!(results[3]["source"]["media_type"], "image/png");
    // A result with no output still says so: the API refuses empty text.
    assert_eq!(turns[4]["content"][0]["content"], "(no output)");
}

#[test]
fn the_prompt_cache_is_marked_at_the_system_the_context_the_last_turn_and_the_one_before() {
    let body = sent(&conversation(), &tools(), Effort::High, &[]);
    let marks = body.to_string().matches("cache_control").count();
    assert_eq!(marks, 4, "the API takes four at most: {body}");
    let turns = &body["messages"];
    assert!(body["system"][0]["cache_control"].is_object());
    assert!(
        turns[0]["content"][0]["cache_control"].is_object(),
        "the context"
    );
    assert!(
        turns[0]["content"][1]["cache_control"].is_null(),
        "not the goal"
    );
    // Where the previous request ended (its last turn): read back whole now.
    assert!(turns[2]["content"][3]["cache_control"].is_object());
    assert!(turns[4]["content"][0]["cache_control"].is_object());
}

#[test]
fn thinking_refused_as_bound_to_another_history_is_left_out() {
    let body = sent(&conversation(), &tools(), Effort::High, &["sig-1"]);
    let said = body["messages"][1]["content"].as_array().unwrap();
    assert_eq!(said[0]["text"], "Looking.");
    assert!(!body.to_string().contains("sig-1"));
}

#[test]
fn a_turn_goes_back_in_the_order_the_model_wrote_it_with_its_calls_as_they_are_now() {
    let think = |sig: &str| json!({"type": "thinking", "thinking": "", "signature": sig});
    let messages = [
        Message::User("go".into()),
        Message::Assistant {
            text: None,
            calls: vec![
                call("c1", "bash", json!({"command": "ls"})),
                // Elided since: what the conversation holds now goes back.
                call("c2", "bash", json!({"command": "[elided]"})),
            ],
            carried: vec![json!({"model": "claude-x", BLOCKS: [
                think("a"), {"type": "tool_use", "id": "c1"},
                think("b"), {"type": "tool_use", "id": "c2"}]})],
        },
        Message::ToolResult {
            id: "c1".into(),
            content: "x".into(),
            failed: false,
        },
        Message::ToolResult {
            id: "c2".into(),
            content: "y".into(),
            failed: false,
        },
    ];
    let body = sent(&messages, &tools(), Effort::High, &[]);
    let said = &body["messages"][1]["content"];
    let kinds: Vec<&str> = (said.as_array().unwrap().iter())
        .map(|block| block["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["thinking", "tool_use", "thinking", "tool_use"]);
    assert_eq!(said[3]["input"]["command"], "[elided]");
    // A block refused once is not sent again; the rest stay in their places.
    let body = sent(&messages, &tools(), Effort::High, &["a"]);
    let said = &body["messages"][1]["content"];
    assert_eq!(said[0]["type"], "tool_use");
    assert_eq!(said[1]["signature"], "b");
    assert_eq!(signatures(&body), ["b"]);
}

#[test]
fn the_deepest_levels_get_room_for_their_thinking() {
    let messages = [Message::User("hi".into())];
    assert_eq!(
        sent(&messages, &[], Effort::High, &[])["max_tokens"],
        32_000
    );
    let deep = selection("m", vec![Effort::High, Effort::Max], Effort::Max);
    let request = Request {
        conversation: &messages,
        tools: &[],
        calling: Calling::Any,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    let thinking = body(&deep, Some(Thinking::Adaptive), &request, &HashSet::new());
    assert_eq!(thinking["max_tokens"], 64_000);
    assert!(
        body(&deep, None, &request, &HashSet::new())
            .get("thinking")
            .is_none(),
        "asked with no thinking"
    );
}

#[test]
fn the_level_is_the_effort_a_current_model_thinks_at_and_a_budget_for_an_older_one() {
    let messages = [Message::User("hi".into())];
    let adaptive = sent(&messages, &[], Effort::High, &[]);
    assert_eq!(adaptive["thinking"], json!({"type": "adaptive"}));
    assert_eq!(adaptive["output_config"], json!({"effort": "high"}));
    assert!(adaptive.get("tools").is_none());
    let off = selection("m", vec![Effort::Off, Effort::Low], Effort::Off);
    let request = Request {
        conversation: &messages,
        tools: &[],
        calling: Calling::Any,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    let body_off = body(&off, Some(Thinking::Adaptive), &request, &HashSet::new());
    assert_eq!(body_off["thinking"], json!({"type": "disabled"}));
    let older = selection("m", vec![Effort::Low, Effort::Medium], Effort::Medium);
    let budget = body(&older, Some(Thinking::Budget), &request, &HashSet::new());
    assert_eq!(
        budget["thinking"],
        json!({"type": "enabled", "budget_tokens": 8192})
    );
    assert!(budget.get("output_config").is_none());
    let plain = selection("m", vec![], Effort::Off);
    assert!(
        body(&plain, Some(Thinking::Adaptive), &request, &HashSet::new())
            .get("thinking")
            .is_none()
    );
}

#[test]
fn a_call_asked_for_alone_is_left_to_the_model_as_the_current_models_refuse_a_forced_one() {
    let messages = [Message::User("now the data".into())];
    let tools = tools();
    let selection = selection("claude-x", vec![], Effort::Off);
    let ask = |calling| {
        let request = Request {
            conversation: &messages,
            tools: &tools,
            calling,
            deadline: None,
            effort: None,
            task: TaskId::NONE,
        };
        body(
            &selection,
            Some(Thinking::Adaptive),
            &request,
            &HashSet::new(),
        )["tool_choice"]
            .clone()
    };
    assert_eq!(ask(Calling::Only("result")), json!({"type": "auto"}));
    assert_eq!(ask(Calling::Nothing), json!({"type": "none"}));
    let request = Request {
        conversation: &messages,
        tools: &tools,
        calling: Calling::Any,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    let offered = body(
        &selection,
        Some(Thinking::Adaptive),
        &request,
        &HashSet::new(),
    );
    assert_eq!(
        offered["tools"][1]["input_schema"]["required"],
        json!(["source"])
    );
    assert_eq!(offered["stream"], true);
}
