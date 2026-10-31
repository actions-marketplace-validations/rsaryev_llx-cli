use clap::CommandFactory;
use clap::error::ErrorKind;

use super::*;

fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("llx").chain(args.iter().copied()))
}

#[test]
fn the_command_line_is_well_formed() {
    Cli::command().debug_assert();
}

#[test]
fn a_prompt_runs_with_its_options() {
    let cli = parse(&[
        "-y",
        "-C",
        "/tmp",
        "--provider",
        "openai",
        "-m",
        "openai/gpt-5-mini",
        "--reasoning",
        "high",
        "--json",
        "-o",
        "last.txt",
        "--max-turns",
        "7",
        "fix the tests",
    ])
    .unwrap();
    assert_eq!(joined(&cli.prompt).as_deref(), Some("fix the tests"));
    assert!(cli.command.is_none());
    let run = cli.run;
    assert!(run.yes && run.json);
    assert_eq!(run.cd, Some(PathBuf::from("/tmp")));
    assert_eq!(run.models.provider.as_deref(), Some("openai"));
    assert_eq!(run.models.model.as_deref(), Some("openai/gpt-5-mini"));
    assert_eq!(run.models.reasoning.as_deref(), Some("high"));
    assert_eq!(run.output_last_message, Some(PathBuf::from("last.txt")));
    assert_eq!(run.max_turns, Some(7));
    let plain = parse(&["x"]).unwrap().run.models;
    assert!(plain.provider.is_none() && plain.model.is_none() && plain.reasoning.is_none());
}

#[test]
fn no_prompt_is_the_prompt_line() {
    let cli = parse(&[]).unwrap();
    assert!(cli.prompt.is_empty() && cli.command.is_none());
    assert_eq!(joined(&parse(&["-"]).unwrap().prompt).as_deref(), Some("-"));
}

#[test]
fn subcommands_parse() {
    // Signing in is `/login` in a session now: `login` is no command, only a word.
    let login = parse(&["login"]).unwrap();
    assert!(login.command.is_none() && login.prompt == ["login"]);
    assert!(matches!(
        parse(&["logout"]).unwrap().command,
        Some(Command::Logout)
    ));
    assert!(matches!(
        parse(&["sessions"]).unwrap().command,
        Some(Command::Sessions)
    ));
    let Some(Command::Doctor(args)) = parse(&["doctor", "-m", "x/y"]).unwrap().command else {
        panic!("doctor")
    };
    assert_eq!(args.models.model.as_deref(), Some("x/y"));
}

#[test]
fn the_decision_model_is_doctors_alone() {
    let Some(Command::Doctor(args)) = parse(&["doctor", "--decision-model", "a/b"])
        .unwrap()
        .command
    else {
        panic!("doctor")
    };
    assert_eq!(args.decision_model, "a/b");
    // A session and `exec` never read it: they do not take it.
    for words in [
        &["--decision-model", "a/b", "x"][..],
        &["exec", "--decision-model", "a/b", "x"],
    ] {
        assert!(parse(words).is_err(), "{words:?}");
    }
}

#[test]
fn resume_takes_an_id_or_the_last_session() {
    let resume = |args: &[&str]| match parse(args).unwrap().command {
        Some(Command::Resume(resume)) => resume,
        _ => panic!("not resume"),
    };
    // The session and the prompt, owned.
    let target = |args: &[&str]| {
        let parsed = resume(args);
        let (id, prompt) = parsed.target();
        (id.map(String::from), prompt)
    };
    let owned = |id: Option<&str>, prompt: &str| (id.map(String::from), Some(prompt.to_string()));
    let by_id = resume(&["resume", "1700000000-ab12", "go on", "-y"]);
    assert!(by_id.run.yes);
    let id = Some("1700000000-ab12");
    assert_eq!(
        target(&["resume", "1700000000-ab12", "go on"]),
        owned(id, "go on")
    );
    assert_eq!(
        target(&["resume", "--last", "now the README"]),
        owned(None, "now the README")
    );
    assert_eq!(target(&["resume", "--last"]), (None, None));
    // Quotes are optional here too: the words are the prompt.
    assert_eq!(target(&["resume", "--last", "a", "b"]), owned(None, "a b"));
    let words = ["resume", "--last", "now", "update", "the", "readme"];
    assert_eq!(target(&words), owned(None, "now update the readme"));
    let words = ["resume", "1700000000-ab12", "go", "on", "-y"];
    assert_eq!(target(&words), owned(id, "go on"));
    let missing = parse(&["resume"]).unwrap_err();
    assert_eq!(missing.kind(), ErrorKind::MissingRequiredArgument);
}

#[test]
fn removed_and_unknown_flags_are_usage_errors() {
    for flag in [
        "--expect",
        "--no-llm",
        "--dry-run",
        "--keep-open",
        "--headless",
        "--trace",
    ] {
        let error = parse(&[flag, "x"]).unwrap_err();
        assert!(error.use_stderr(), "{flag}");
    }
    let zero = parse(&["--max-turns", "0", "x"]).unwrap_err();
    assert_eq!(zero.kind(), ErrorKind::ValueValidation);
    assert!(zero.to_string().contains("1..=10000"), "{zero}");
    assert!(!zero.to_string().contains("18446744073709551615"));
    assert!(parse(&["--timeout", "86401", "x"]).is_err());
    assert!(parse(&["--timeout", "86400", "x"]).is_ok());
    for bad in ["0", "-1", "abc", "nan", "inf"] {
        assert!(parse(&["--budget", bad, "x"]).is_err(), "{bad}");
    }
    let ok = parse(&["--budget", "0.50", "x"]).unwrap();
    assert_eq!(ok.run.budget, Some(0.5));
}

#[test]
fn exec_runs_one_task() {
    for name in ["exec", "e"] {
        let Some(Command::Exec(exec)) = parse(&[name, "--json", "-y", "fix it"]).unwrap().command
        else {
            panic!("{name} is not exec")
        };
        assert_eq!(joined(&exec.prompt).as_deref(), Some("fix it"));
        assert!(exec.run.json && exec.run.yes && exec.command.is_none());
    }
    let Some(Command::Exec(exec)) = parse(&["exec", "resume", "--last", "go on"])
        .unwrap()
        .command
    else {
        panic!("exec resume")
    };
    let Some(ExecCommand::Resume(resume)) = exec.command else {
        panic!("not exec resume")
    };
    let (id, prompt) = resume.target();
    assert_eq!((id, prompt.as_deref()), (None, Some("go on")));
}

#[test]
fn print_and_continue_are_flags() {
    let print = parse(&["-p", "review this"]).unwrap();
    assert!(print.print && !print.resume_last);
    assert_eq!(joined(&print.prompt).as_deref(), Some("review this"));
    let resumed = parse(&["-c"]).unwrap();
    assert!(resumed.resume_last && resumed.prompt.is_empty());
    assert!(parse(&["-c", "-p", "go on"]).unwrap().print);
    for alias in ["--yolo", "--dangerously-skip-permissions"] {
        assert!(parse(&[alias, "x"]).unwrap().run.yes, "{alias}");
    }
}

#[test]
fn the_mode_is_a_flag_with_its_other_spellings_and_conflicts_with_yes() {
    assert_eq!(
        parse(&["--mode", "plan", "x"]).unwrap().run.mode,
        Some("plan".to_string())
    );
    let Some(Command::Exec(exec)) = parse(&["exec", "--permission-mode", "bypass", "x"])
        .unwrap()
        .command
    else {
        panic!("not exec")
    };
    assert_eq!(exec.run.mode.as_deref(), Some("bypass"));
    assert_eq!(parse(&["x"]).unwrap().run.mode, None);
    let conflict = parse(&["-y", "--mode", "plan", "x"]).unwrap_err();
    assert_eq!(conflict.kind(), ErrorKind::ArgumentConflict);
}

#[test]
fn a_command_counts_only_as_the_first_word() {
    // After the first word of a prompt, a command's name is just another word of it.
    let cli = parse(&["fix it", "sessions"]).unwrap();
    assert_eq!(joined(&cli.prompt).as_deref(), Some("fix it sessions"));
    assert!(cli.command.is_none());
    assert!(parse(&["logout", "status"]).is_err());
}

#[test]
fn the_help_shows_the_usage_the_examples_and_where_things_are_kept() {
    let help = Cli::command().render_long_help().to_string();
    assert!(help.contains("Usage: llx [OPTIONS] [PROMPT]"), "{help}");
    assert!(help.contains("Examples:"), "{help}");
    // Signing in is a slash command of a session, not a command of its own.
    assert!(
        help.contains("/login") && !help.contains("llx login"),
        "{help}"
    );
    // The default sessions directory, as `commands::sessions::store` builds it.
    assert!(help.contains("~/.llx/sessions"), "{help}");
    let version = Cli::command().render_version();
    assert!(version.starts_with("llx "), "{version}");
}

#[test]
fn help_and_version_are_not_errors() {
    for flag in ["--help", "-h", "--version", "-V"] {
        let shown = parse(&[flag]).unwrap_err();
        assert!(!shown.use_stderr(), "{flag}");
    }
}

#[test]
fn a_prompt_needs_no_quotes_and_commands_still_parse() {
    let cli = parse(&["make", "a", "landing", "page", "-y"]).unwrap();
    assert_eq!(joined(&cli.prompt).as_deref(), Some("make a landing page"));
    assert!(cli.run.yes, "an option after the words is still an option");
    assert!(cli.command.is_none());
    let Some(Command::Exec(exec)) = parse(&["exec", "fix", "the", "tests"]).unwrap().command else {
        panic!("exec")
    };
    assert_eq!(joined(&exec.prompt).as_deref(), Some("fix the tests"));
}

#[test]
fn the_help_says_how_sessions_run_side_by_side() {
    let help = Cli::command().render_long_help().to_string();
    for said in ["/new", "Left arrow", "Exit and stop tasks", "x stops"] {
        assert!(help.contains(said), "{said}: {help}");
    }
}
