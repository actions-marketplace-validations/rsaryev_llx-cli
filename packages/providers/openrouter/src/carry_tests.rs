use serde_json::json;

use super::*;

#[test]
fn nothing_is_carried_for_a_plain_turn() {
    let message = json!({"content": "Done.", "tool_calls": [{"id": "c1",
        "function": {"name": "read", "arguments": "{}"}}]});
    assert_eq!(carried(&message, "m"), None);
}
