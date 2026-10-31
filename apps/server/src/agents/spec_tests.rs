use serde_json::json;

use super::*;

#[test]
fn each_action_is_read_with_what_it_needs() {
    assert_eq!(
        action(&json!({"action": "spawn", "task": " look ", "name": "API Scout"})),
        Ok(Action::Spawn {
            task: "look".into(),
            name: Some("api-scout".into()),
            context: Context::Fork,
        })
    );
    assert!(matches!(
        action(&json!({"action": "spawn", "task": "look", "context": "Fresh"})),
        Ok(Action::Spawn {
            context: Context::Fresh,
            ..
        })
    ));
    assert_eq!(
        action(&json!({"action": "send", "agent": "scout", "message": "go on"})),
        Ok(Action::Send {
            agent: "scout".into(),
            message: "go on".into()
        })
    );
    assert_eq!(action(&json!({"action": "list"})), Ok(Action::List));
    assert_eq!(
        action(&json!({"action": "close", "agent": "scout"})),
        Ok(Action::Close {
            agent: "scout".into()
        })
    );
}

#[test]
fn a_call_without_its_action_is_read_from_what_it_gives() {
    assert!(matches!(
        action(&json!({"task": "look"})),
        Ok(Action::Spawn { .. })
    ));
    assert!(matches!(
        action(&json!({"agent": "scout", "message": "more"})),
        Ok(Action::Send { .. })
    ));
    assert!(action(&json!({})).unwrap_err().contains("action"));
}

#[test]
fn only_the_names_the_schema_gives_are_read() {
    let refused = |args| action(&args).unwrap_err();
    for other in [
        "start", "run", "message", "continue", "status", "stop", "end",
    ] {
        let call = json!({"action": other, "task": "t", "agent": "a", "message": "m"});
        assert!(
            refused(call).contains(&format!("no action `{other}`")),
            "{other}"
        );
    }
    // Each action reads its own keys alone.
    assert!(refused(json!({"action": "spawn", "prompt": "look"})).contains("`task`"));
    assert!(refused(json!({"action": "spawn", "message": "look"})).contains("`task`"));
    assert!(refused(json!({"action": "send", "id": "a", "message": "m"})).contains("`agent`"));
    assert!(refused(json!({"action": "send", "name": "a", "message": "m"})).contains("`agent`"));
    assert!(refused(json!({"action": "send", "agent": "a", "task": "m"})).contains("`message`"));
    assert!(refused(json!({"action": "close", "name": "a"})).contains("`agent`"));
    assert!(refused(json!({"prompt": "look"})).contains("`action`"));
    assert!(refused(json!({"id": "a", "message": "m"})).contains("`action`"));
    let context = json!({"action": "spawn", "task": "t", "context": "new"});
    assert!(refused(context).contains("`fork` or `fresh`"));
    // Whatever their case.
    assert_eq!(action(&json!({"action": "LIST"})), Ok(Action::List));
}

#[test]
fn what_is_missing_is_said() {
    let missing = |args| action(&args).unwrap_err();
    assert!(missing(json!({"action": "spawn"})).contains("`task`"));
    assert!(missing(json!({"action": "send", "agent": "a"})).contains("`message`"));
    assert!(missing(json!({"action": "close"})).contains("`agent`"));
    assert!(missing(json!({"action": "fly"})).contains("no action `fly`"));
}

#[test]
fn a_name_is_tidied_or_dropped() {
    assert_eq!(tidy("  Test Runner #2 "), Some("test-runner-2".into()));
    assert_eq!(tidy("!!!"), None);
    assert_eq!(tidy(&"x".repeat(40)).unwrap().len(), 24);
}
