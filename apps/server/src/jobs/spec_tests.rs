use serde_json::json;

use super::*;

/// What a session reads of a call.
fn action_of(arguments: &Value) -> Result<Action, String> {
    action(arguments, true)
}

#[test]
fn each_action_is_read_with_what_it_needs() {
    let monitor = action_of(&json!({"action": "monitor", "command": "test -f x", "name": "Done"}));
    assert_eq!(
        monitor,
        Ok(Action::Monitor {
            command: "test -f x".into(),
            name: Some("done".into()),
            every: Duration::from_secs(10),
            timeout: Duration::from_millis(300_000),
            needs: Needs::Sandbox,
        })
    );
    let escalated = action_of(&json!({"action": "monitor", "command": "x", "escalate": true}));
    assert!(matches!(
        escalated,
        Ok(Action::Monitor {
            needs: Needs::Open,
            ..
        })
    ));
    let schedule = action_of(&json!({"action": "schedule", "prompt": "check", "in": "45m"}));
    assert!(matches!(
        schedule,
        Ok(Action::Schedule {
            when: When::In(2700),
            recurring: false,
            ..
        })
    ));
    let cron = action_of(&json!({
        "action": "schedule", "prompt": "check", "cron": "*/5 * * * *", "recurring": true
    }));
    assert!(matches!(
        cron,
        Ok(Action::Schedule {
            when: When::Cron(_),
            recurring: true,
            ..
        })
    ));
    assert_eq!(action_of(&json!({"action": "list"})), Ok(Action::List));
    assert_eq!(
        action_of(&json!({"action": "stop", "job": "logs"})),
        Ok(Action::Stop { job: "logs".into() })
    );
}

#[test]
fn what_is_wrong_is_said() {
    let wrong = |args| action_of(&args).unwrap_err();
    assert!(wrong(json!({"action": "monitor"})).contains("`command`"));
    assert!(wrong(json!({"action": "schedule", "prompt": "p"})).contains("`cron` or `in`"));
    assert!(
        wrong(json!({"action": "schedule", "prompt": "p", "cron": "* *"})).contains("5 fields")
    );
    let once = json!({"action": "schedule", "prompt": "p", "in": "5m", "recurring": true});
    assert!(wrong(once).contains("needs `cron`"));
    assert!(wrong(json!({"action": "fly"})).contains("no action `fly`"));
}

#[test]
fn only_the_names_the_schema_gives_are_read() {
    let wrong = |args| action_of(&args).unwrap_err();
    assert!(wrong(json!({"action": "watch", "command": "x"})).contains("no action `watch`"));
    let cron = json!({"action": "cron", "prompt": "p", "cron": "* * * * *"});
    assert!(wrong(cron).contains("no action `cron`"));
    assert!(wrong(json!({"action": "cancel", "job": "j"})).contains("no action `cancel`"));
    assert!(wrong(json!({"action": "stop", "name": "j"})).contains("`job`"));
    // Whatever their case.
    assert_eq!(action_of(&json!({"action": "List"})), Ok(Action::List));
}

#[test]
fn a_watch_is_bounded() {
    let long = action_of(&json!({"action": "monitor", "command": "x", "timeout_ms": 99_999_999}));
    assert!(matches!(long, Ok(Action::Monitor { timeout, .. }) if timeout.as_secs() == 1800));
    // A check every second would be a busy loop: five seconds apart at least.
    let often = action_of(&json!({"action": "monitor", "command": "x", "every": 1}));
    assert!(matches!(often, Ok(Action::Monitor { every, .. }) if every.as_secs() == 5));
}
