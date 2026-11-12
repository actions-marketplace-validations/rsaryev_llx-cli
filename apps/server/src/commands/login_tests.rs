use crate::choice::Level;
use std::sync::Mutex;

use contracts::catalog::Effort;

use super::*;
use crate::testing::Scripted;

/// What the steps chose, and each sign-in they asked for.
type Outcome = (Result<Ended, String>, Vec<(Provider, Method)>);

/// Runs the steps with OpenAI signed in or not.
fn steps(current: &Choice, openai_signed_in: bool, terminal: &mut Scripted) -> Outcome {
    run_steps(current, openai_signed_in, terminal, true)
}

/// [`steps`], with the sign-in the person's cancel (`signs` false) or not.
fn run_steps(
    current: &Choice,
    openai_signed_in: bool,
    terminal: &mut Scripted,
    signs: bool,
) -> Outcome {
    let asked = Mutex::new(vec![]);
    let signed_in = |provider: &Provider| {
        (*provider == Provider::openai() && openai_signed_in)
            .then(|| "ChatGPT account a".to_string())
    };
    let sign_in = |provider, method, _: &str, _: &mut dyn Terminal| {
        asked.lock().unwrap().push((provider, method));
        Ok(signs.then(|| "Signed in.".to_string()))
    };
    let world = World {
        signed_in: &signed_in,
        sign_in: &sign_in,
        say: &|_| {},
    };
    let chosen = choose(current, terminal, &world);
    (chosen, asked.into_inner().unwrap())
}

fn chosen_of(ended: Result<Ended, String>) -> Choice {
    match ended {
        Ok(Ended::Chose(choice)) => choice,
        other => panic!("{other:?}"),
    }
}

fn openrouter() -> Choice {
    Choice::new(Provider::openrouter(), "anthropic/claude-haiku-5.5")
}

#[test]
fn signing_in_to_openai_then_choosing_a_model_and_a_level_it_takes() {
    // OpenAI, the ChatGPT sign-in, then its default model and level together.
    let models = Provider::openai().models();
    let luna = models.iter().position(|m| m.id == "gpt-6-luna").unwrap();
    let mut terminal = Scripted::new(&[Some(0), Some(0), Some(luna)]);
    let (chosen, asked) = steps(&openrouter(), false, &mut terminal);
    let Ok(Ended::Chose(chosen)) = chosen else {
        panic!("{chosen:?}")
    };
    assert_eq!(asked, [(Provider::openai(), Method::Browser)]);
    assert_eq!(
        terminal.titles()[..3],
        [
            "Login › Provider",
            "Login › OpenAI › Sign in with",
            "Login › OpenAI › Model"
        ]
    );
    // How OpenAI is signed in to is said where the provider is picked, and nowhere else.
    let (_, providers) = &terminal.shown[0];
    let openai = providers
        .iter()
        .find(|item| item.label == "OpenAI")
        .unwrap();
    assert_eq!(
        openai.detail,
        "openai · not signed in · ChatGPT sign-in, or an API key"
    );
    assert_eq!(chosen.provider, Provider::openai());
    assert_eq!(chosen.model(), "gpt-6-luna");
    let marked = terminal
        .offered("Login › OpenAI › Model")
        .into_iter()
        .position(|(_, marked)| marked);
    assert_eq!(marked, Some(luna), "the default is the one marked");
    assert_eq!(terminal.titles().len(), 3, "no separate reasoning menu");
    assert_eq!(chosen.selection.effort(), models[luna].default);
}

#[test]
fn the_model_list_keeps_the_current_level_or_starts_at_the_new_models_default() {
    for (effort, model) in [(Effort::Max, "gpt-5.5"), (Effort::High, "gpt-6-sol")] {
        let current = Choice::new(Provider::openai(), "gpt-6-sol");
        let current = Choice {
            selection: current.selection.with_effort(effort).unwrap(),
            level: Level::Fixed,
            ..current
        };
        let models = Provider::openai().models();
        let at = models.iter().position(|m| m.id == model).unwrap();
        // OpenAI (current), keep the sign-in, confirm the model and inline level.
        let mut terminal = Scripted::new(&[Some(0), Some(0), Some(at)]);
        let (chosen, asked) = steps(&current, true, &mut terminal);
        assert!(asked.is_empty(), "{model}: the sign-in was kept");
        let expected = if model == current.model() {
            effort
        } else {
            models[at].default
        };
        assert_eq!(terminal.titles().len(), 3);
        let chosen = chosen_of(chosen);
        assert_eq!(chosen.selection.effort(), expected, "{model}");
        // Another model starts at `auto`: its default until a level is chosen for it.
        let level = if model == current.model() {
            Level::Fixed
        } else {
            Level::Auto
        };
        assert_eq!(chosen.level, level, "{model}");
    }
}

#[test]
fn openrouter_asks_how_to_sign_in_and_a_model_with_one_level_skips_the_step() {
    let current = Choice::new(Provider::openrouter(), "vendor/own-model");
    // OpenRouter (fourth of the popular), paste a key, the current (custom) model listed first.
    let at = Provider::all()
        .iter()
        .position(|p| *p == Provider::openrouter())
        .unwrap();
    let mut terminal = Scripted::new(&[Some(at), Some(0), Some(0)]);
    let (chosen, asked) = steps(&current, false, &mut terminal);
    assert_eq!(asked, [(Provider::openrouter(), Method::Key)]);
    assert_eq!(
        terminal.offered("Login › OpenRouter › Sign in with")[0],
        ("Paste an API key".into(), true)
    );
    assert!(
        !terminal
            .titles()
            .iter()
            .any(|title| title.starts_with("Reasoning"))
    );
    let chosen = chosen_of(chosen);
    assert_eq!(
        (chosen.model(), chosen.selection.effort()),
        ("vendor/own-model", Effort::Off)
    );
}

#[test]
fn cancelling_the_first_step_ends_the_flow_with_nothing_changed() {
    let mut terminal = Scripted::new(&[None]);
    assert_eq!(
        steps(&openrouter(), true, &mut terminal).0,
        Ok(Ended::Nothing)
    );
}

#[test]
fn every_provider_is_offered_with_the_signed_in_marked_and_none_typed_by_id() {
    let mut terminal = Scripted::new(&[None]);
    let (chosen, _) = steps(&openrouter(), true, &mut terminal);
    assert_eq!(chosen, Ok(Ended::Nothing));
    let (_, items) = &terminal.shown[0];
    // Every provider at once, none behind a row to open; then the one row that adds a host.
    assert_eq!(items.len(), Provider::all().len() + 1);
    let added: Vec<usize> = (0..items.len())
        .filter(|at| items[*at].label.starts_with("Another"))
        .collect();
    assert_eq!(added, [items.len() - 1]);
    let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
    assert_eq!(labels[..2], ["OpenAI ✓", "Anthropic"], "{labels:?}");
    // The id leads the detail: a search finds a provider by it.
    assert_eq!(items[0].detail, "openai · signed in: ChatGPT account a");
}

#[test]
fn the_popular_come_first_then_the_current_and_signed_in_then_the_rest_by_name() {
    let all: Vec<Provider> = ["zeta", "openai", "acme", "deepseek", "beta", "mid"]
        .iter()
        .map(|id| Provider(id.to_string()))
        .collect();
    let signed_in = |p: &Provider| (p.id() == "beta").then(|| "a key".to_string());
    let order: Vec<String> = offered(all, &Provider("mid".into()), &signed_in)
        .iter()
        .map(|p| p.id().to_string())
        .collect();
    assert_eq!(order, ["openai", "deepseek", "beta", "mid", "acme", "zeta"]);
}

#[test]
fn cancelling_a_step_goes_back_to_the_one_before_it_as_it_was() {
    // OpenAI, not signed in: cancel "Sign in with", the providers again with OpenAI marked,
    // then OpenAI, its sign-in, a model.
    let mut terminal = Scripted::new(&[Some(0), None, Some(0), Some(0), Some(0)]);
    let (chosen, asked) = steps(&openrouter(), false, &mut terminal);
    assert!(matches!(chosen, Ok(Ended::Chose(_))), "{chosen:?}");
    assert_eq!(asked, [(Provider::openai(), Method::Browser)]);
    let titles = terminal.titles();
    assert!(titles[1].ends_with("Sign in with"), "{titles:?}");
    assert_eq!(titles[2], titles[0], "{titles:?}");
    let marked = terminal.shown[2].1.iter().position(|item| item.marked);
    assert_eq!(marked, Some(0), "OpenAI, the one chosen, is marked");
}

#[test]
fn cancelling_the_model_after_keeping_a_sign_in_goes_back_to_keep_or_sign_in() {
    let mut terminal = Scripted::new(&[Some(0), Some(0), None, Some(0), Some(0)]);
    let (chosen, asked) = steps(&openrouter(), true, &mut terminal);
    assert!(matches!(chosen, Ok(Ended::Chose(_))), "{chosen:?}");
    assert!(asked.is_empty());
    let titles = terminal.titles();
    assert!(
        titles[1].ends_with("Signed in with ChatGPT account a"),
        "{titles:?}"
    );
    assert!(titles[2].ends_with("Model"), "{titles:?}");
    assert_eq!(titles[3], titles[1], "{titles:?}");
}

#[test]
fn cancelling_a_sign_in_goes_back_to_how_to_sign_in_marked_as_chosen() {
    // OpenAI, the API key (the second way), cancelled: the ways again with it marked, then the
    // person gives up at the providers.
    let mut terminal = Scripted::new(&[Some(0), Some(1), None, None]);
    let (chosen, asked) = run_steps(&openrouter(), false, &mut terminal, false);
    assert_eq!(chosen, Ok(Ended::Nothing));
    assert_eq!(asked, [(Provider::openai(), Method::Key)]);
    let titles = terminal.titles();
    assert_eq!(titles[1], titles[2], "{titles:?}");
    let marked = terminal.shown[2].1.iter().position(|item| item.marked);
    assert_eq!(marked, Some(1));
    assert_eq!(titles[3], "Login › Provider");
}

#[test]
fn backing_out_of_the_model_after_signing_in_goes_to_the_providers_and_says_what_is_true() {
    let mut terminal = Scripted::new(&[Some(0), Some(0), None, None]);
    let (chosen, asked) = steps(&openrouter(), false, &mut terminal);
    assert_eq!(chosen, Ok(Ended::SignedIn(Provider::openai())));
    assert_eq!(asked.len(), 1, "the sign-in is not asked twice");
    let titles = terminal.titles();
    assert_eq!(titles[2], "Login › OpenAI › Model");
    assert_eq!(titles[3], "Login › Provider");
}
