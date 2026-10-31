use super::*;

/// [`super::resolve`] with no key in any variable: the same on every machine.
fn resolve(options: &ModelOptions, saved: Option<Choice>) -> Result<Choice, Failure> {
    super::resolve(options, saved, Some(&[]))
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "a test helper takes its arguments as the tests write them, by value"
)]
fn options(provider: Provider, model: Option<&str>, reasoning: Option<&str>) -> ModelOptions {
    ModelOptions {
        provider: Some(provider.id().to_string()),
        model: model.map(str::to_string),
        reasoning: reasoning.map(str::to_string),
    }
}

fn at(choice: Choice, effort: Effort) -> Choice {
    let selection = choice.selection.with_effort(effort).unwrap();
    Choice {
        selection,
        level: Level::Fixed,
        ..choice
    }
}

#[test]
fn a_saved_choice_reads_back_and_a_level_its_model_lost_falls_to_the_default() {
    let choice = at(Choice::new(Provider::openai(), "gpt-6-sol"), Effort::Max);
    assert_eq!(Choice::from_json(&choice.to_json()), Some(choice));
    let stale = json!({"provider": "openai", "model": "gpt-5.5", "reasoning": "max"});
    let read = Choice::from_json(&stale).unwrap();
    assert_eq!(
        read.selection.effort(),
        Effort::Medium,
        "gpt-5.5 tops out at xhigh"
    );
    assert_eq!(
        Choice::from_json(&json!({"provider": "elsewhere", "model": "m"})),
        None
    );
}

#[test]
fn with_nothing_given_the_saved_choice_of_that_provider_is_used() {
    let saved = at(Choice::new(Provider::openai(), "gpt-6-sol"), Effort::High);
    let resolved = resolve(
        &options(Provider::openai(), None, None),
        Some(saved.clone()),
    );
    assert_eq!(resolved, Ok(saved.clone()));
    let other = resolve(&options(Provider::openrouter(), None, None), Some(saved)).unwrap();
    assert_eq!(other.model(), "anthropic/claude-haiku-5.5");
}

#[test]
fn another_model_starts_at_its_own_default() {
    let saved = at(Choice::new(Provider::openai(), "gpt-6-sol"), Effort::Max);
    let resolved = resolve(
        &options(Provider::openai(), Some("gpt-5.5"), None),
        Some(saved),
    )
    .unwrap();
    assert_eq!(
        (resolved.model(), resolved.selection.effort()),
        ("gpt-5.5", Effort::Medium)
    );
}

#[test]
fn a_level_the_model_does_not_take_is_a_usage_error() {
    let given = options(Provider::openai(), Some("gpt-5.5"), Some("xhigh"));
    assert_eq!(
        resolve(&given, None).unwrap().selection.effort(),
        Effort::XHigh
    );
    let refused = resolve(
        &options(Provider::openai(), Some("gpt-5.5"), Some("max")),
        None,
    );
    assert_eq!(
        refused,
        Err(Failure::Usage(
            "--reasoning max: gpt-5.5 takes off, low, medium, high, xhigh".into()
        ))
    );
    let unknown = resolve(
        &options(Provider::openrouter(), Some("v/m"), Some("low")),
        None,
    );
    assert!(matches!(unknown, Err(Failure::Usage(m)) if m.ends_with("takes off")));
}

#[test]
fn saving_a_choice_keeps_the_rest_of_the_settings() {
    let choice = Choice::new(Provider::openai(), "gpt-6-sol");
    let before = r#"{"modes":{"review":{"allow":["bash"]}},"model":"old"}"#;
    let kept: Value = serde_json::from_str(&merged(Some(before), &choice)).unwrap();
    assert_eq!(kept["modes"]["review"]["allow"][0], "bash");
    assert_eq!(kept["model"], "gpt-6-sol");
    let fresh: Value = serde_json::from_str(&merged(Some("not json"), &choice)).unwrap();
    assert_eq!(fresh["provider"], "openai");
}

#[test]
fn a_level_neither_given_nor_saved_is_auto_and_one_saved_by_name_stays() {
    let new = Choice::new(Provider::openai(), "gpt-6-sol");
    assert_eq!(new.level, Level::Auto);
    assert!(new.label().ends_with("· auto"), "{}", new.label());
    assert_eq!(new.to_json()["reasoning"], "auto");
    assert_eq!(Choice::from_json(&new.to_json()), Some(new));
    let unsaved = json!({"provider": "openai", "model": "gpt-6-sol"});
    assert_eq!(Choice::from_json(&unsaved).unwrap().level, Level::Auto);
    let named = json!({"provider": "openai", "model": "gpt-6-sol", "reasoning": "high"});
    let named = Choice::from_json(&named).unwrap();
    assert_eq!(
        (named.level, named.selection.effort()),
        (Level::Fixed, Effort::High)
    );
}

#[test]
fn reasoning_auto_on_the_command_line_leaves_the_level_to_the_decision_model() {
    let saved = at(Choice::new(Provider::openai(), "gpt-6-sol"), Effort::High);
    let auto = resolve(
        &options(Provider::openai(), None, Some("auto")),
        Some(saved.clone()),
    );
    assert_eq!(auto.unwrap().level, Level::Auto);
    let low = resolve(&options(Provider::openai(), None, Some("low")), Some(saved)).unwrap();
    assert_eq!(
        (low.level, low.selection.effort()),
        (Level::Fixed, Effort::Low)
    );
}
