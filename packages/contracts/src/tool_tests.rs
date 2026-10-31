use serde_json::json;

use super::*;

#[test]
fn a_report_alone_is_an_output_that_shows_nothing_more() {
    let output = ToolOutput::from("done".to_string());
    assert_eq!(output.report, "done");
    assert_eq!(
        output,
        ToolOutput {
            report: "done".into(),
            ..ToolOutput::default()
        }
    );
}

#[test]
fn a_text_tool_offered_as_a_function_takes_its_text_in_one_string_argument() {
    let text = Input::Text("start: /.+/".into());
    let schema = text.schema();
    assert_eq!(schema["required"], json!([TEXT_ARGUMENT]));
    assert_eq!(schema["properties"][TEXT_ARGUMENT]["type"], "string");
    // A JSON tool's schema is its own.
    let own = json!({"type": "object", "properties": {"path": {"type": "string"}}});
    assert_eq!(Input::Json(own.clone()).schema(), own);
}

#[test]
fn a_function_call_of_a_text_tool_is_unwrapped_to_its_text_and_wrapped_again_to_go_back() {
    let text = Input::Text("start: /.+/".into());
    let wire = json!({"source": "return 1"});
    assert_eq!(text.from_function(wire.clone()), json!("return 1"));
    assert_eq!(text.to_function(&json!("return 1")), wire);
    // What is not that shape stays as it is: the tool says what is wrong with it.
    assert_eq!(text.from_function(json!("{\"sour")), json!("{\"sour"));
    assert_eq!(text.from_function(json!({"other": 1})), json!({"other": 1}));
    // A JSON tool's arguments are never touched.
    let json_tool = Input::Json(json!({}));
    assert_eq!(json_tool.from_function(wire.clone()), wire);
    assert_eq!(json_tool.to_function(&wire), wire);
}

#[test]
fn a_call_is_readable_when_its_arguments_have_the_shape_the_input_takes() {
    assert!(Input::Json(json!({})).accepts(&json!({"a": 1})));
    assert!(!Input::Json(json!({})).accepts(&json!("text")));
    assert!(Input::Text("start: /.+/".into()).accepts(&json!("text")));
    assert!(!Input::Text("start: /.+/".into()).accepts(&json!({"source": "x"})));
}
