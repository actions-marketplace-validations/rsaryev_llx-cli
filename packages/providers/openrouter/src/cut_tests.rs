use serde_json::json;

use super::*;

#[test]
fn the_string_a_call_stopped_in_is_closed_and_the_object_after_it() {
    assert_eq!(
        closed(r#"{"source": "line one\nline \"two"#),
        Some(json!({"source": "line one\nline \"two"}))
    );
    assert_eq!(
        closed(r#"{"a": [{"b": "x"#),
        Some(json!({"a": [{"b": "x"}]}))
    );
}

#[test]
fn an_escape_cut_midway_is_left_out() {
    for (raw, kept) in [
        (r#"{"source": "a\"#, "a"),
        (r#"{"source": "a\u00"#, "a"),
        (r#"{"source": "a\ud83d"#, "a"),
        (r#"{"source": "a\ud83d\ude"#, "a"),
        (r#"{"source": "a\\"#, "a\\"),
    ] {
        assert_eq!(closed(raw), Some(json!({"source": kept})), "{raw}");
    }
}

#[test]
fn arguments_that_did_not_stop_in_a_string_are_not_made_whole() {
    assert_eq!(closed(r#"{"source": "a", "n": 1"#), None);
    assert_eq!(closed(r#"{"sour"#), None);
    assert_eq!(closed(""), None);
}
