use super::*;
use crate::mode::Reach;

#[test]
fn the_persons_modes_follow_the_built_in_ones_and_may_replace_one() {
    let text = r#"{"modes": {"review": {"allow": ["bash(cargo test *)"], "description": "Review."},
        "plan": {"allow": ["bash(cargo test *)"]}, "look": {"reach": "read-only"}}}"#;
    let modes = Modes::with_settings(Some(text), "settings.json").unwrap();
    assert_eq!(modes.names(), "default, plan, bypass, look, review");
    let plan = modes.named("plan").unwrap();
    assert_eq!(
        plan.rules().judge("bash", "cargo test -q"),
        crate::rules::Ruling::Allowed
    );
    assert_eq!(modes.named("review").unwrap().hint(), "Review.");
    // A mode is read-only when it says so: one that replaces `plan` and does not, is not.
    assert_eq!(modes.named("look").unwrap().reach(), Reach::ReadOnly);
    assert_eq!(plan.reach(), Reach::Anything);
    assert_eq!(modes.named("review").unwrap().reach(), Reach::Anything);
    assert_eq!(
        Modes::built_in().named("plan").unwrap().reach(),
        Reach::ReadOnly
    );
}

#[test]
fn a_persons_mode_says_what_its_commands_reach() {
    let text = r#"{"modes": {"work": {"reach": "workspace"}, "all": {"reach": "anything"},
        "look": {"reach": "read-only"}}}"#;
    let modes = Modes::with_settings(Some(text), "settings.json").unwrap();
    assert_eq!(modes.named("work").unwrap().reach(), Reach::Workspace);
    assert_eq!(modes.named("all").unwrap().reach(), Reach::Anything);
    assert_eq!(modes.named("look").unwrap().reach(), Reach::ReadOnly);
    assert_eq!(built("default").reach(), Reach::Workspace);
    let text = r#"{"modes": {"odd": {"reach": "everywhere"}}}"#;
    let why = Modes::with_settings(Some(text), "settings.json").unwrap_err();
    assert!(why.contains("`odd`") && why.contains("workspace"), "{why}");
}

#[test]
fn the_older_read_only_key_is_refused_naming_reach() {
    // Read as no reach at all, `read_only: true` would let the mode reach anything.
    for value in ["true", "false"] {
        let text = format!(r#"{{"modes": {{"odd": {{"read_only": {value}}}}}}}"#);
        let why = Modes::with_settings(Some(&text), "settings.json").unwrap_err();
        assert!(
            why.contains("`odd`") && why.contains("\"read-only\""),
            "{why}"
        );
    }
}

#[test]
fn settings_with_no_modes_or_not_json_add_none() {
    for text in [None, Some("not json"), Some(r#"{"provider":"openai"}"#)] {
        assert_eq!(
            Modes::with_settings(text, "s").unwrap().names(),
            "default, plan, bypass"
        );
    }
}

#[test]
fn a_mode_that_cannot_be_read_names_the_file_and_the_mode() {
    for text in [
        r#"{"modes": {"broken": {"allow": ["bash(echo *"]}}}"#,
        r#"{"modes": {"broken": {"allow": "bash"}}}"#,
        r#"{"modes": {"broken": {"deny": [1]}}}"#,
    ] {
        let error = Modes::with_settings(Some(text), "/c/settings.json").unwrap_err();
        assert!(
            error.contains("/c/settings.json") && error.contains("`broken`"),
            "{error}"
        );
    }
}

#[test]
fn yes_chooses_bypass_nothing_the_default_and_an_unknown_name_lists_them() {
    let modes = Modes::built_in();
    assert_eq!(modes.choose(None, true).unwrap().name(), "bypass");
    assert_eq!(modes.choose(None, false).unwrap().name(), "default");
    let Err(Failure::Usage(why)) = modes.choose(Some("autonomous"), false) else {
        panic!("an unknown mode is a usage error");
    };
    assert!(why.contains("default, plan, bypass"), "{why}");
}

#[test]
fn shift_tab_goes_round_every_mode() {
    let modes = Modes::with_settings(Some(r#"{"modes":{"review":{}}}"#), "s").unwrap();
    let mut now = modes.named("default").unwrap();
    let mut seen = vec![];
    for _ in 0..5 {
        now = modes.after(&now);
        seen.push(now.name().to_string());
    }
    assert_eq!(seen, ["plan", "bypass", "review", "default", "plan"]);
}
