//! Signing out, and a run that is not signed in: it stops before any model.

use crate::common::home::{FAKE_KEY, Home, text};

#[test]
fn logout_forgets_every_saved_sign_in_without_showing_it() {
    let home = Home::new();
    let config = home.path().join("config/llx");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("credentials"), FAKE_KEY).unwrap();
    std::fs::write(config.join("openai.json"), "{\"access_token\": \"secret\"}").unwrap();
    let out = home.run(&["logout"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let shown = text(&out.stdout);
    assert!(shown.contains("OpenRouter: signed out"), "{shown}");
    assert!(shown.contains("OpenAI: signed out"), "{shown}");
    assert!(!shown.contains(FAKE_KEY) && !shown.contains("secret"));
    assert!(!config.join("credentials").exists() && !config.join("openai.json").exists());
    let again = text(&home.run(&["logout"]).stdout);
    assert!(again.contains("No saved sign-in."), "{again}");
}

#[test]
fn a_provider_not_signed_in_and_a_level_the_model_lacks_stop_before_any_model() {
    let home = Home::new();
    // (arguments, whether the OpenRouter key is set, what stderr says)
    let cases: &[(&[&str], bool, &str)] = &[
        (&["exec", "fix the tests"], false, "not signed in"),
        (
            &["exec", "--provider", "openai", "x"],
            true,
            "not signed in",
        ),
        (
            &["exec", "--reasoning", "ultra", "x"],
            true,
            "--reasoning ultra: anthropic/claude-haiku-5.5 takes off, low",
        ),
    ];
    for (args, key, said) in cases {
        let out = match key {
            true => home.run_with_key(args),
            false => home.run(args),
        };
        assert_eq!(out.status.code(), Some(3), "{args:?}");
        let stderr = text(&out.stderr);
        assert!(stderr.contains(said), "{args:?}: {stderr}");
    }
}

#[test]
fn signed_out_says_how_to_sign_in_to_the_provider_in_use() {
    let home = Home::new();
    let cases: &[(&str, &str, &str)] = &[
        (
            "anthropic",
            "not signed in to Anthropic",
            "ANTHROPIC_API_KEY",
        ),
        ("openai", "not signed in to OpenAI", "OPENAI_API_KEY"),
    ];
    for (provider, said, variable) in cases {
        let out = home.run_with_key(&["exec", "--provider", provider, "x"]);
        assert_eq!(out.status.code(), Some(3), "{provider}");
        let stderr = text(&out.stderr);
        assert!(
            stderr.contains(said) && stderr.contains(variable) && stderr.contains("/login"),
            "{provider}: {stderr}"
        );
        assert!(
            !stderr.contains("OPENROUTER_API_KEY"),
            "{provider}: {stderr}"
        );
    }
}
