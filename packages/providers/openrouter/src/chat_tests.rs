use contracts::chat::TaskId;

use super::*;
use contracts::tool::Input;

#[test]
fn a_conversation_goes_out_in_the_wire_format_with_cache_marks() {
    let call = ToolCall {
        id: "c1".into(),
        name: "bash".into(),
        arguments: json!({"command": "ls"}),
    };
    let messages = cached(
        [
            Message::System("rules".into()),
            Message::User("goal".into()),
            Message::Assistant {
                text: None,
                calls: vec![call],
                carried: vec![],
            },
            Message::ToolResult {
                id: "c1".into(),
                content: "a.rs".into(),
                failed: false,
            },
        ]
        .iter()
        .map(|message| wire(message, "m", &[]))
        .collect(),
    );
    assert_eq!(
        messages[0]["content"][0]["cache_control"]["type"],
        "ephemeral"
    );
    assert_eq!(
        messages[1]["content"][0]["cache_control"]["type"], "ephemeral",
        "the context after the system prompt is marked"
    );
    assert_eq!(
        messages[2]["tool_calls"][0]["function"]["arguments"],
        r#"{"command":"ls"}"#
    );
    assert_eq!(messages[3]["tool_call_id"], "c1");
    assert_eq!(messages[3]["content"][0]["text"], "a.rs");
}

#[test]
fn a_tool_call_with_unreadable_arguments_arrives_as_their_text() {
    // A reply cut short leaves `{"command": "cargo te` behind: it must not become an empty call
    // that fails as "missing field command".
    let cut = "{\"command\": \"cargo te";
    let call = read_call(&json!({"id": "c2", "function": {"name": "bash", "arguments": cut}}));
    assert_eq!(call.map(|c| c.arguments), Some(json!(cut)));
}

#[test]
fn a_picture_goes_out_as_a_user_message_with_an_image_part_and_the_mark() {
    let image = Image {
        media_type: "image/jpeg".into(),
        data: "/9j/AA==".into(),
        width: 2,
        height: 1,
    };
    let messages = cached(
        [
            Message::System("rules".into()),
            Message::Image {
                caption: "The screenshot".into(),
                image,
            },
        ]
        .iter()
        .map(|message| wire(message, "m", &[]))
        .collect(),
    );
    let picture = &messages[1];
    assert_eq!(picture["role"], "user");
    assert_eq!(picture["content"][0]["text"], "The screenshot");
    assert_eq!(picture["content"][1]["type"], "image_url");
    assert_eq!(
        picture["content"][1]["image_url"]["url"],
        "data:image/jpeg;base64,/9j/AA=="
    );
    assert_eq!(
        picture["content"][1]["cache_control"]["type"], "ephemeral",
        "the newest message keeps its cache mark"
    );
}

#[test]
fn off_turns_thinking_off_and_a_level_is_asked_by_name() {
    assert_eq!(reasoning(Effort::Off), json!({"enabled": false}));
    assert_eq!(reasoning(Effort::XHigh), json!({"effort": "xhigh"}));
}

fn selection(efforts: Vec<Effort>, effort: Effort) -> Selection {
    selection_of("lab/model", efforts, effort)
}

fn selection_of(id: &str, efforts: Vec<Effort>, effort: Effort) -> Selection {
    let model = contracts::catalog::ModelEntry {
        id: id.into(),
        name: "Model".into(),
        default: efforts.first().copied().unwrap_or(Effort::Off),
        efforts,
        context: 100_000,
        output: 0,
        marks: id.starts_with("marked/"),
    };
    Selection::new(model).with_effort(effort).unwrap()
}

fn request_body(wire: Wire, selection: &Selection, calling: Calling) -> Value {
    let conversation = [
        Message::System("rules".into()),
        Message::User("context".into()),
        Message::User("goal".into()),
    ];
    let tools = [ToolSpec {
        name: "read".into(),
        description: "Read.".into(),
        input: Input::Json(json!({"type": "object"})),
    }];
    let request = Request {
        conversation: &conversation,
        tools: &tools,
        calling,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    body(wire, selection, &request)
}

#[test]
fn openrouter_is_sent_its_reasoning_object_the_usage_report_and_cache_marks() {
    let levels = vec![Effort::Off, Effort::Low, Effort::High];
    let sent = request_body(
        Wire::OpenRouter,
        &selection_of("marked/claude-x", levels.clone(), Effort::High),
        Calling::Any,
    );
    assert_eq!(sent["reasoning"], json!({"effort": "high"}));
    assert_eq!(sent["usage"], json!({"include": true}));
    assert_eq!(
        sent["messages"][0]["content"][0]["cache_control"]["type"],
        "ephemeral"
    );
    assert!(sent.get("reasoning_effort").is_none());
    assert_eq!(sent["tool_choice"], "auto");
    assert_eq!(sent["parallel_tool_calls"], true);
    let off = request_body(
        Wire::OpenRouter,
        &selection(levels.clone(), Effort::Off),
        Calling::Nothing,
    );
    assert_eq!(off["reasoning"], json!({"enabled": false}));
    assert_eq!(off["tool_choice"], "none", "an answer is asked for");
    // The `result` call with `--schema`'s data is asked for by name: the reply is that call.
    let only = request_body(
        Wire::OpenRouter,
        &selection(levels, Effort::Off),
        Calling::Only("result"),
    );
    assert_eq!(
        only["tool_choice"],
        json!({"type": "function", "function": {"name": "result"}})
    );
}

#[test]
fn a_model_with_no_levels_is_sent_no_reasoning_field_on_openrouter_either() {
    let sent = request_body(
        Wire::OpenRouter,
        &selection(vec![], Effort::Off),
        Calling::Any,
    );
    assert!(sent.get("reasoning").is_none(), "{sent}");
    let generic = request_body(Wire::Plain, &selection(vec![], Effort::Off), Calling::Any);
    assert!(generic.get("reasoning_effort").is_none());
}

#[test]
fn another_host_is_sent_reasoning_effort_by_name_and_no_openrouter_fields() {
    let levels = vec![Effort::Off, Effort::Low, Effort::High];
    let sent = request_body(
        Wire::Plain,
        &selection(levels.clone(), Effort::Low),
        Calling::Any,
    );
    assert_eq!(sent["reasoning_effort"], "low");
    assert!(sent.get("reasoning").is_none() && sent.get("usage").is_none());
    assert_eq!(sent["messages"][0]["content"], "rules", "no cache marks");
    let off = request_body(Wire::Plain, &selection(levels, Effort::Off), Calling::Any);
    assert_eq!(off["reasoning_effort"], "none");
}

#[test]
fn openai_itself_is_sent_a_cache_key_per_conversation_and_others_none() {
    // OpenAI routes by the key: one shared by every session puts them all on one machine, past
    // what it takes, and the rest read the conversation afresh. Each conversation keys its own.
    let key = |wire: Wire, conversation: &[Message]| {
        let request = Request {
            conversation,
            tools: &[],
            calling: Calling::Any,
            deadline: None,
            effort: None,
            task: TaskId::NONE,
        };
        body(wire, &selection(vec![], Effort::Off), &request)["prompt_cache_key"].clone()
    };
    let opened = |goal: &str| {
        vec![
            Message::System("rules".into()),
            Message::User("context".into()),
            Message::User(goal.into()),
        ]
    };
    let first = opened("one goal");
    let own = key(Wire::OpenAi, &first);
    assert!(own.is_string(), "{own}");
    let mut later = first.clone();
    later.extend([
        Message::Assistant {
            text: Some("Done.".into()),
            calls: vec![],
            carried: vec![],
        },
        Message::User("and the tests".into()),
    ]);
    assert_eq!(key(Wire::OpenAi, &later), own, "a follow-up keeps it");
    assert_ne!(key(Wire::OpenAi, &opened("another goal")), own);
    // A host that may refuse a field it does not know is sent none; OpenRouter has its marks.
    assert_eq!(key(Wire::Plain, &first), Value::Null);
    assert_eq!(key(Wire::OpenRouter, &first), Value::Null);
}

#[test]
fn the_models_reasoning_goes_back_to_it_with_the_next_request() {
    // A model that thinks between its tool calls needs its reasoning back, whole: OpenRouter's
    // `reasoning_details` (signed or sealed parts), else the text in the field it came in.
    let details = json!([{"type": "reasoning.encrypted", "data": "sealed", "id": "r1",
        "format": "anthropic-claude-v1", "index": 0}]);
    let reply = json!({"model": "m", "choices": [{"message": {"content": null,
        "reasoning": "thinking", "reasoning_details": details,
        "tool_calls": [{"id": "c1", "type": "function",
            "function": {"name": "read", "arguments": "{}"}}]}}]});
    let completion = read(&reply, "m", 0).unwrap();
    let sent = wire(&completion.message(), "m", &[]);
    assert_eq!(sent["reasoning_details"], details);
    assert!(
        sent.get("reasoning").is_none(),
        "the details stand for the text"
    );
    assert_eq!(sent["tool_calls"][0]["id"], "c1");

    // A host that says it in `reasoning_content` (DeepSeek's own API) is answered in it.
    let reply = json!({"model": "m", "choices": [{"message": {"content": "Done.",
        "reasoning_content": "step by step"}}]});
    let sent = wire(&read(&reply, "m", 0).unwrap().message(), "m", &[]);
    assert_eq!(sent["reasoning_content"], "step by step");

    // Another model (after /model) is sent none of it.
    let other = wire(&completion.message(), "other", &[]);
    assert!(other.get("reasoning_details").is_none() && other.get("reasoning").is_none());
}

#[test]
fn cache_marks_go_only_to_the_models_that_take_explicit_breakpoints() {
    let marked = |id: &str| {
        let sent = request_body(
            Wire::OpenRouter,
            &selection_of(id, vec![], Effort::Off),
            Calling::Any,
        );
        sent.to_string().contains("cache_control")
    };
    // The catalog says which models are marked (`ModelEntry::marks`), not the id.
    assert!(marked("marked/model"));
    assert!(!marked("anthropic/by-name-only"));
    let sent = request_body(
        Wire::OpenRouter,
        &selection_of("lab/plain", vec![], Effort::Off),
        Calling::Any,
    );
    assert_eq!(
        sent["messages"][0]["content"], "rules",
        "the content stays a plain string"
    );
}

/// A tool of free text, as `apply_patch` is.
fn patch() -> ToolSpec {
    ToolSpec {
        name: "apply_patch".into(),
        description: "Change files.".into(),
        input: Input::Text("start: /.+/".into()),
    }
}

#[test]
fn a_patch_is_offered_as_a_function_with_one_string_argument() {
    let conversation = [Message::User("goal".into())];
    let tools = [patch()];
    let request = Request {
        conversation: &conversation,
        tools: &tools,
        calling: Calling::Any,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    let body = body(Wire::OpenRouter, &selection(vec![], Effort::Off), &request);
    let function = &body["tools"][0]["function"];
    assert_eq!(function["name"], "apply_patch");
    assert_eq!(function["parameters"]["required"], json!(["source"]));
    assert_eq!(
        function["parameters"]["properties"]["source"]["type"],
        "string"
    );
}

#[test]
fn a_patch_call_is_its_text_coming_in_and_its_one_argument_going_back() {
    let wired = json!({"id": "c1", "function": {"name": "apply_patch",
        "arguments": "{\"source\":\"*** Begin Patch\"}"}});
    let call = read_call(&wired).unwrap();
    // The reader keeps what the model wrote; the port takes the text out for the agent.
    let tools = [patch()];
    let call = incoming(call, &tools);
    assert_eq!(call.arguments, json!("*** Begin Patch"));

    let sent = wire(
        &Message::Assistant {
            text: None,
            calls: vec![call],
            carried: vec![],
        },
        "m",
        &tools,
    );
    let arguments = sent["tool_calls"][0]["function"]["arguments"]
        .as_str()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(arguments).unwrap(),
        json!({"source": "*** Begin Patch"})
    );
}

#[test]
fn a_reply_cut_at_the_output_limit_says_so() {
    let reply = |reason: &str| {
        json!({"model": "m", "choices": [{"finish_reason": reason,
            "message": {"content": "The fix is"}}]})
    };
    let cut = read(&reply("length"), "m", 0).unwrap();
    assert_eq!(cut.ending, Ending::Cut);
    assert_eq!(cut.text.as_deref(), Some("The fix is"));
    assert_eq!(read(&reply("stop"), "m", 0).unwrap().ending, Ending::Whole);
}

#[test]
fn openrouter_groups_a_conversations_requests_into_one_session() {
    // OpenRouter's sessions: its logs show them together, and it routes them where the cache is.
    let selection = selection_of("lab/model", vec![], Effort::Off);
    let session = |wire: Wire, conversation: &[Message]| {
        let request = Request {
            conversation,
            tools: &[],
            calling: Calling::Any,
            deadline: None,
            effort: None,
            task: TaskId::NONE,
        };
        body(wire, &selection, &request)["session_id"].clone()
    };
    let opened = |goal: &str| {
        vec![
            Message::System("rules".into()),
            Message::User("context".into()),
            Message::User(goal.into()),
        ]
    };
    let first = opened("Goal: fix the parser");
    let id = session(Wire::OpenRouter, &first);
    assert!(
        id.as_str()
            .is_some_and(|id| !id.is_empty() && id.len() <= 256),
        "{id}"
    );
    let mut later = first.clone();
    later.extend([
        Message::Assistant {
            text: Some("Fixed.".into()),
            calls: vec![],
            carried: vec![],
        },
        Message::User("Goal: now test it".into()),
    ]);
    assert_eq!(session(Wire::OpenRouter, &later), id, "a follow-up's too");
    assert_ne!(session(Wire::OpenRouter, &opened("Goal: another")), id);
    // Another host may refuse a field it does not know.
    assert_eq!(session(Wire::Plain, &first), Value::Null);
}

#[test]
fn a_reply_cut_off_while_the_model_thought_gives_its_thought_as_text() {
    // As OpenRouter streams a reply its output limit cut in the middle of the reasoning.
    let event = |delta: Value, finish: Value| json!({"model": "m", "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]});
    let part = |text: &str| json!([{"type": "reasoning.text", "text": text, "index": 0}]);
    let body: String = [
        event(
            json!({"role": "assistant", "reasoning": "Plan: read a.rs, ",
                "reasoning_details": part("Plan: read a.rs, ")}),
            Value::Null,
        ),
        event(
            json!({"reasoning": "then patch it.", "reasoning_details": part("then patch it.")}),
            Value::Null,
        ),
        event(json!({}), json!("length")),
    ]
    .iter()
    .map(|event| format!("data: {event}\n\n"))
    .chain(["data: [DONE]\n\n".to_string()])
    .collect();
    let cut = read(&crate::stream::assembled(&body).unwrap(), "m", 0).unwrap();
    assert_eq!(cut.ending, Ending::Cut);
    assert!(cut.text.is_none() && cut.calls.is_empty(), "{cut:?}");
    assert_eq!(
        cut.thought.as_deref(),
        Some("Plan: read a.rs, then patch it.")
    );

    // A reasoning only sealed has no text to give.
    let sealed = json!({"model": "m", "choices": [{"finish_reason": "length", "message": {
        "content": null, "reasoning_details": [{"type": "reasoning.encrypted", "data": "x"}]}}]});
    assert_eq!(read(&sealed, "m", 0).unwrap().thought, None);
}

#[test]
fn a_reply_is_held_to_32000_tokens_or_the_models_own_smaller_limit() {
    // OpenRouter sizes a budgeted model's thinking from it (Anthropic: a share of max_tokens):
    // with none, a medium level thought its way through the model's whole limit.
    let unknown = request_body(
        Wire::OpenRouter,
        &selection(vec![], Effort::Off),
        Calling::Any,
    );
    assert_eq!(unknown["max_tokens"], json!(32_000));
    let mut small = selection(vec![], Effort::Off).model().clone();
    small.output = 8_192;
    let sent = request_body(Wire::Plain, &Selection::new(small), Calling::Any);
    assert_eq!(sent["max_tokens"], json!(8_192));
}

#[test]
fn openai_itself_is_held_to_the_limit_by_the_field_its_reasoning_models_take() {
    // OpenAI's reasoning models refuse `max_tokens`: every chat model of its takes the other.
    let sent = request_body(
        Wire::OpenAi,
        &selection(vec![Effort::Low], Effort::Low),
        Calling::Any,
    );
    assert_eq!(sent["max_completion_tokens"], json!(32_000));
    assert_eq!(sent.get("max_tokens"), None);
}

#[test]
fn openrouter_is_asked_to_stream_and_another_host_is_not() {
    let levels = vec![Effort::Low];
    let streamed = request_body(
        Wire::OpenRouter,
        &selection(levels.clone(), Effort::Low),
        Calling::Any,
    );
    assert_eq!(streamed["stream"], json!(true));
    let whole = request_body(Wire::Plain, &selection(levels, Effort::Low), Calling::Any);
    assert_eq!(whole.get("stream"), None);
}

#[test]
fn a_cut_patch_call_keeps_the_text_it_wrote_and_another_cut_call_its_raw_arguments() {
    let reply = |name: &str, arguments: &str| {
        json!({"model": "m", "choices": [{"finish_reason": "length", "message": {"content": null,
            "tool_calls": [{"id": "c1", "function": {"name": name, "arguments": arguments}}]}}]})
    };
    let bash = ToolSpec {
        name: "bash".into(),
        description: "Run.".into(),
        input: Input::Json(json!({"type": "object"})),
    };
    let tools = [patch(), bash];
    let cut = arrived(
        read(
            &reply("apply_patch", r#"{"source": "*** Begin Patch\n*** Add"#),
            "m",
            0,
        )
        .unwrap(),
        &tools,
    );
    assert_eq!(cut.calls[0].arguments, json!("*** Begin Patch\n*** Add"));
    // A command cut off midway is not made to look whole.
    let raw = r#"{"command": "rm -r build/ca"#;
    let cut = arrived(read(&reply("bash", raw), "m", 0).unwrap(), &tools);
    assert_eq!(cut.calls[0].arguments, json!(raw));
}
