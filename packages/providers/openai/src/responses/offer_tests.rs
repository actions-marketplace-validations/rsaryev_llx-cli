use contracts::catalog::{Effort, ModelEntry, Selection};
use contracts::chat::{Calling, Request, TaskId};

use super::super::{request, route};
use super::*;

/// The request body as `complete` builds it, its tools offered as `texts` says.
fn request_body(selection: &Selection, request: Request, texts: Texts) -> Value {
    let offer = Offer::new(request.tools, texts, request.conversation);
    let session = route::session(request.conversation);
    request::body(selection, request, &offer, &session)
}

const GRAMMAR: &str = "start: /.+/";
const PATCH: &str = "*** Begin Patch\n*** Delete File: a\n*** End Patch\n";

/// A tool of free text, as `apply_patch` is.
fn patch() -> ToolSpec {
    ToolSpec {
        name: "apply_patch".into(),
        description: "Change files".into(),
        input: Input::Text(GRAMMAR.into()),
    }
}

fn read_tool() -> ToolSpec {
    ToolSpec {
        name: "read".into(),
        description: "Read".into(),
        input: Input::Json(json!({"type": "object"})),
    }
}

fn selection() -> Selection {
    Selection::new(ModelEntry {
        id: "gpt-6-luna".into(),
        name: "luna".into(),
        efforts: vec![],
        default: Effort::Off,
        context: 400_000,
        output: 0,
        marks: false,
    })
}

/// A conversation in which the model patched a file and read one, and both answered.
fn conversation() -> Vec<Message> {
    vec![
        Message::User("goal".into()),
        Message::Assistant {
            text: None,
            calls: vec![
                ToolCall {
                    id: "s1".into(),
                    name: "apply_patch".into(),
                    arguments: json!(PATCH),
                },
                ToolCall {
                    id: "r1".into(),
                    name: "read".into(),
                    arguments: json!({"path": "a"}),
                },
            ],
            carried: vec![],
        },
        Message::ToolResult {
            id: "s1".into(),
            content: "Done".into(),
            failed: false,
        },
        Message::ToolResult {
            id: "r1".into(),
            content: "missing".into(),
            failed: true,
        },
    ]
}

fn sent(texts: Texts) -> Value {
    let conversation = conversation();
    let tools = [patch(), read_tool()];
    let request = Request {
        conversation: &conversation,
        tools: &tools,
        calling: Calling::Any,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    request_body(&selection(), request, texts)
}

#[test]
fn a_model_that_takes_custom_tools_is_offered_the_patch_with_its_grammar() {
    let body = sent(Texts::Custom);
    assert_eq!(
        body["tools"][0],
        json!({"type": "custom", "name": "apply_patch", "description": "Change files",
            "format": {"type": "grammar", "syntax": "lark", "definition": GRAMMAR}})
    );
    // The other tools stay functions.
    assert_eq!(body["tools"][1]["type"], "function");
    assert_eq!(body["tools"][1]["name"], "read");
}

#[test]
fn any_other_model_is_offered_the_patch_as_a_function_with_one_string_argument() {
    let body = sent(Texts::Functions);
    let patch = &body["tools"][0];
    assert_eq!(patch["type"], "function");
    assert_eq!(patch["parameters"]["required"], json!(["source"]));
    assert_eq!(
        patch["parameters"]["properties"]["source"]["type"],
        "string"
    );
}

#[test]
fn an_earlier_patch_and_its_answer_go_back_as_custom_items_only_when_offered_so() {
    let body = sent(Texts::Custom);
    let input = body["input"].as_array().unwrap();
    assert_eq!(
        input[1],
        json!({"type": "custom_tool_call", "call_id": "s1", "name": "apply_patch",
            "input": PATCH})
    );
    assert_eq!(input[2]["type"], "function_call");
    assert_eq!(
        input[3],
        json!({"type": "custom_tool_call_output", "call_id": "s1", "output": "Done"})
    );
    assert_eq!(
        input[4],
        json!({"type": "function_call_output", "call_id": "r1", "output": "Error: missing"})
    );

    let body = sent(Texts::Functions);
    let input = body["input"].as_array().unwrap();
    let arguments: Value = serde_json::from_str(input[1]["arguments"].as_str().unwrap()).unwrap();
    assert_eq!(input[1]["type"], "function_call");
    assert_eq!(arguments, json!({"source": PATCH}));
    assert_eq!(input[3]["type"], "function_call_output");
}

#[test]
fn a_custom_call_is_read_as_the_text_the_model_wrote() {
    let item = json!({"type": "custom_tool_call", "call_id": "c9", "name": "apply_patch",
        "input": PATCH});
    let call = custom_call(&item).unwrap();
    assert_eq!(call.id, "c9");
    assert_eq!(call.name, "apply_patch");
    assert_eq!(call.arguments, json!(PATCH));
    assert!(custom_call(&json!({"type": "custom_tool_call", "name": "apply_patch"})).is_none());
}

#[test]
fn a_function_call_of_a_patch_is_taken_as_its_text() {
    let specs = [patch(), read_tool()];
    let offer = Offer::new(&specs, Texts::Functions, &[]);
    let call = |name: &str, arguments: Value| ToolCall {
        id: "c".into(),
        name: name.into(),
        arguments,
    };
    assert_eq!(
        offer
            .incoming(call("apply_patch", json!({"source": PATCH})))
            .arguments,
        json!(PATCH)
    );
    let read = json!({"source": "kept"});
    assert_eq!(offer.incoming(call("read", read.clone())).arguments, read);
}
