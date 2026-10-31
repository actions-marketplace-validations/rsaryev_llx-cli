use super::*;

/// A client at `/work` that started with `args`, in an environment of `env`.
fn place(args: &[&str], env: &[(&str, &str)]) -> Place {
    Place {
        cwd: "/work".into(),
        args: args.iter().map(|arg| arg.to_string()).collect(),
        env: (env.iter())
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect(),
        dir: "/work".into(),
    }
}

/// The model options and the advice switch the client's session runs with.
fn chosen(args: &[&str], env: &[(&str, &str)]) -> (Option<String>, Option<String>, bool) {
    let mut cli = command_line(&place(args, env)).unwrap();
    let options = cli.options();
    let models = &options.models;
    (
        models.provider.clone(),
        models.model.clone(),
        options.no_advice,
    )
}

const ENV: [(&str, &str); 4] = [
    ("LLX_PROVIDER", "openrouter"),
    ("LLX_MODEL", "anthropic/claude-haiku-5.5"),
    ("LLX_REASONING", "low"),
    ("HOME", "/home/a"),
];

#[test]
fn the_model_the_clients_environment_names_is_the_sessions_unless_its_command_line_says_another() {
    let mut cli = command_line(&place(&["resume", "1-ab"], &ENV)).unwrap();
    let models = &cli.options().models;
    assert_eq!(
        (models.provider.as_deref(), models.model.as_deref()),
        (Some("openrouter"), Some("anthropic/claude-haiku-5.5"))
    );
    assert_eq!(models.reasoning.as_deref(), Some("low"));
    // What the command line says wins, in any of its spellings.
    let given = ["-mx/y", "--reasoning=high", "--provider", "deepseek"];
    let mut cli = command_line(&place(&given, &ENV)).unwrap();
    let models = &cli.options().models;
    assert_eq!(
        (models.provider.as_deref(), models.model.as_deref()),
        (Some("deepseek"), Some("x/y"))
    );
    assert_eq!(models.reasoning.as_deref(), Some("high"));
    // An empty variable names nothing.
    assert_eq!(chosen(&[], &[("LLX_MODEL", "")]).1, None);
}

#[test]
fn a_model_given_among_clustered_flags_is_the_command_lines() {
    let (_, model, _) = chosen(&["-ym", "x/y"], &ENV);
    assert_eq!(model.as_deref(), Some("x/y"));
}

#[test]
fn a_prompt_word_that_reads_like_an_option_sets_nothing() {
    let mut cli = command_line(&place(&["--", "-mfoo", "--model"], &ENV)).unwrap();
    assert_eq!(cli.prompt, ["-mfoo", "--model"]);
    let model = cli.options().models.model.clone();
    assert_eq!(model.as_deref(), Some("anthropic/claude-haiku-5.5"));
}

#[test]
fn the_clients_no_advice_is_the_sessions() {
    assert!(chosen(&[], &[("LLX_NO_ADVICE", "1")]).2);
    assert!(!chosen(&[], &[("LLX_NO_ADVICE", "off")]).2);
    assert!(!chosen(&[], &[]).2);
    assert!(chosen(&["--no-advice"], &[("LLX_NO_ADVICE", "0")]).2);
}
