use contracts::catalog::Effort;
use contracts::chat::TaskId;

use super::*;
use contracts::tool::Input;

use crate::level::Switch;

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
    assert_eq!(level::reasoning(Effort::Off), json!({"enabled": false}));
    assert_eq!(level::reasoning(Effort::XHigh), json!({"effort": "xhigh"}));
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
    body_with(wire, selection, calling, crate::level::Switch::Named)
}

fn selection_with_output(efforts: Vec<Effort>, effort: Effort, output: u64) -> Selection {
    let model = contracts::catalog::ModelEntry {
        id: "lab/model".into(),
        name: "Model".into(),
        default: efforts[0],
        efforts,
        context: 100_000,
        output,
        marks: false,
    };
    Selection::new(model).with_effort(effort).unwrap()
}

fn body_with(
    wire: Wire,
    selection: &Selection,
    calling: Calling,
    switch: crate::level::Switch,
) -> Value {
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
    body(wire, selection, &request, switch)
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
    // A call asked for by name goes out as `auto` over chat completions: hosts refuse a named
    // choice while their model thinks (DeepSeek, Qwen), take only `auto` (Z.ai), or refuse one
    // for their newest models (Claude); the agent asks for the call in words, and `result` is
    // offered with every request. OpenAI's own API forces it.
    for host in [Wire::OpenRouter, Wire::Plain] {
        let only = request_body(
            host,
            &selection(levels.clone(), Effort::Off),
            Calling::Only("result"),
        );
        assert_eq!(only["tool_choice"], "auto");
    }
    let openai = request_body(
        Wire::OpenAi,
        &selection(vec![], Effort::Off),
        Calling::Only("result"),
    );
    assert_eq!(
        openai["tool_choice"],
        json!({"type": "function", "function": {"name": "result"}})
    );
}

#[test]
fn a_host_with_a_toggle_of_its_own_is_sent_it_for_off() {
    use crate::level::{Switch, Toggle};
    let levels = || vec![Effort::Off, Effort::Medium];
    let sent = |switch, effort| {
        body_with(
            Wire::Plain,
            &selection(levels(), effort),
            Calling::Any,
            switch,
        )
    };
    let toggled = Switch::Toggled(Toggle::Thinking);
    let off = sent(toggled, Effort::Off);
    assert_eq!(off["thinking"], json!({"type": "disabled"}));
    assert!(off.get("reasoning_effort").is_none());
    assert_eq!(
        sent(toggled, Effort::Medium)["thinking"],
        json!({"type": "enabled"})
    );
    let qwen = Switch::NamedOrOff(Toggle::EnableThinking);
    assert_eq!(sent(qwen, Effort::Off)["enable_thinking"], false);
    let on = sent(qwen, Effort::Medium);
    assert_eq!(on["reasoning_effort"], "medium");
    assert!(on.get("enable_thinking").is_none());
    assert_eq!(sent(Switch::Named, Effort::Off)["reasoning_effort"], "none");
}

#[test]
fn the_deepest_levels_may_write_64000_tokens() {
    // OpenRouter sizes a budgeted model's thinking from the output limit (95% at xhigh and max):
    // at 32,000 a reply had a few thousand tokens left for its calls.
    let sent = |effort| {
        let levels = vec![Effort::High, Effort::XHigh];
        let selection = selection_with_output(levels, effort, 128_000);
        request_body(Wire::OpenRouter, &selection, Calling::Any)["max_tokens"].clone()
    };
    assert_eq!(sent(Effort::High), 32_000);
    assert_eq!(sent(Effort::XHigh), 64_000);
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
        body(
            wire,
            &selection(vec![], Effort::Off),
            &request,
            Switch::Named,
        )["prompt_cache_key"]
            .clone()
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
fn a_calls_thought_signature_goes_back_on_the_call_as_gemini_requires() {
    let signed = json!({"google": {"thought_signature": "sig=="}});
    let reply = json!({"model": "g", "choices": [{"message": {"content": null,
        "tool_calls": [{"id": "c1", "type": "function", "extra_content": signed,
            "function": {"name": "read", "arguments": "{}"}},
            {"id": "c2", "type": "function", "function": {"name": "read", "arguments": "{}"}}]}}]});
    let completion = read(&reply, "g", 0).unwrap();
    let sent = wire(&completion.message(), "g", &[]);
    assert_eq!(sent["tool_calls"][0]["extra_content"], signed);
    assert!(sent["tool_calls"][1].get("extra_content").is_none());
    assert!(
        wire(&completion.message(), "other", &[])["tool_calls"][0]
            .get("extra_content")
            .is_none()
    );
}

#[test]
fn an_answer_in_chunks_is_read_as_its_text_and_goes_back_whole() {
    // Mistral's reasoning models answer in chunks, their thinking among them.
    let chunks = json!([{"type": "thinking", "thinking": [{"type": "text", "text": "hmm"}]},
        {"type": "text", "text": "Done."}]);
    let reply = json!({"model": "m", "choices": [{"message": {"content": chunks}}]});
    let completion = read(&reply, "m", 0).unwrap();
    assert_eq!(completion.text.as_deref(), Some("Done."));
    assert_eq!(wire(&completion.message(), "m", &[])["content"], chunks);
}

#[test]
fn a_withheld_reply_is_an_error_and_a_hosts_own_cache_count_is_read() {
    let filtered = json!({"model": "m", "choices": [{"finish_reason": "content_filter",
        "message": {"content": ""}}]});
    assert!(read(&filtered, "m", 0).is_err());
    // Together's: at the top of the usage.
    let reply = json!({"model": "m", "choices": [{"message": {"content": "ok"}}],
        "usage": {"prompt_tokens": 100, "cached_tokens": 60, "completion_tokens": 1}});
    assert_eq!(read(&reply, "m", 0).unwrap().usage.cached_tokens, 60);
}

#[test]
fn what_the_cache_read_and_wrote_is_taken_from_wherever_a_host_says_it() {
    let usage = |usage: Value| {
        let reply = json!({"choices": [{"message": {"content": "ok"}}], "usage": usage});
        read(&reply, "m", 0).unwrap().usage
    };
    // DeepSeek's own API.
    let deepseek = usage(json!({"prompt_tokens": 100, "prompt_cache_hit_tokens": 70,
        "prompt_cache_miss_tokens": 30, "completion_tokens": 1}));
    assert_eq!(deepseek.cached_tokens, 70);
    // A relay that passes on Anthropic's own counts.
    let relayed = usage(json!({"prompt_tokens": 100, "cache_read_input_tokens": 80,
        "cache_creation_input_tokens": 15, "completion_tokens": 1}));
    assert_eq!((relayed.cached_tokens, relayed.written_tokens), (80, 15));
    // A zero in the usual place hides no count said elsewhere.
    let twice = usage(json!({"prompt_tokens": 100, "prompt_cache_hit_tokens": 70,
        "prompt_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 0},
        "cache_creation_input_tokens": 9}));
    assert_eq!((twice.cached_tokens, twice.written_tokens), (70, 9));
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
    let body = body(
        Wire::OpenRouter,
        &selection(vec![], Effort::Off),
        &request,
        Switch::Named,
    );
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
        body(wire, &selection, &request, Switch::Named)["session_id"].clone()
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

#[test]
fn a_reply_on_the_persons_own_provider_key_costs_what_the_provider_charged_too() {
    let reply = json!({"model": "m", "choices": [{"message": {"content": "ok"}}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 1, "cost": 0.0001, "is_byok": true,
            "cost_details": {"upstream_inference_cost": 0.002}}});
    let cost = read(&reply, "m", 0).unwrap().usage.cost;
    assert!((cost - 0.0021).abs() < 1e-9, "{cost}");
}

#[test]
fn a_local_host_is_asked_to_stream_with_its_usage_and_no_other_hosts_field() {
    let model = contracts::catalog::ModelEntry {
        id: "gemma-4-12b".into(),
        name: "gemma-4-12b".into(),
        efforts: vec![],
        default: Effort::Off,
        context: 16_384,
        output: 4_096,
        marks: false,
    };
    let sent = request_body(Wire::Local, &Selection::new(model), Calling::Any);
    // Streamed: a reply the limit cuts keeps its last call's text (B7), and the usage comes only
    // when the stream is asked for it.
    assert_eq!(sent["stream"], json!(true));
    assert_eq!(sent["stream_options"], json!({"include_usage": true}));
    assert_eq!(sent["max_tokens"], json!(4_096), "the host's own limit");
    for other in ["usage", "session_id", "reasoning", "prompt_cache_key"] {
        assert_eq!(sent.get(other), None, "{other}");
    }
    assert_eq!(sent["messages"][0]["content"], "rules", "no cache mark");
    assert_eq!(sent["tools"][0]["function"]["name"], "read");
}

#[test]
fn a_local_hosts_stream_is_read_with_its_usage_and_what_it_took_from_its_cache() {
    // As llama.cpp's server streams it: the deltas, the finish with its timings, then the usage
    // alone in an event with no choice.
    let stream = concat!(
        "data: {\"choices\":[{\"finish_reason\":null,\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null}}],\"model\":\"gemma-4-12b\"}\n\n",
        "data: {\"choices\":[{\"finish_reason\":null,\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"type\":\"function\",\"function\":{\"name\":\"bash\",\"arguments\":\"{\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"finish_reason\":null,\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"command\\\":\\\"ls\\\"}\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"finish_reason\":\"tool_calls\",\"index\":0,\"delta\":{}}],\"timings\":{\"cache_n\":3900,\"prompt_n\":90,\"predicted_n\":30}}\n\n",
        "data: {\"choices\":[],\"usage\":{\"completion_tokens\":30,\"prompt_tokens\":3990,\"total_tokens\":4020}}\n\n",
        "data: [DONE]\n\n",
    );
    let whole = crate::stream::assembled(stream).unwrap();
    let completion = read(&whole, "gemma-4-12b", 5).unwrap();
    assert_eq!(completion.calls[0].arguments, json!({"command": "ls"}));
    assert_eq!(completion.usage.input_tokens, 3_990);
    assert_eq!(completion.usage.output_tokens, 30);
    assert_eq!(completion.usage.cached_tokens, 3_900);
    assert_eq!(completion.ending, Ending::Whole);
}
