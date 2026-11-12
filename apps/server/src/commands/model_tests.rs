use contracts::catalog::{Effort, Selection};

use super::*;
use crate::choice::Level;
use crate::testing::Scripted;

fn openrouter() -> Choice {
    Choice::new(Provider::openrouter(), "anthropic/claude-haiku-5.5")
}

#[test]
fn model_offers_signed_in_models_and_confirms_without_a_second_menu() {
    let current = openrouter();
    let openai_only =
        |provider: &Provider| (*provider == Provider::openai()).then(|| "a".to_string());
    let mut terminal = Scripted::new(&[None]);
    assert_eq!(
        pick_model(&current, &mut terminal, &openai_only, &[], None),
        None
    );
    let offered = terminal.offered("Model");
    let marked = offered.iter().filter(|(_, m)| *m).count();
    assert_eq!(marked, 1, "the current model is marked: {offered:?}");
    let openai_models = Provider::openai().models().len();
    let openrouter_models = Provider::openrouter().models().len();
    assert_eq!(
        offered.len(),
        openai_models + openrouter_models,
        "the current provider's models, and those of every provider signed in to"
    );
    // One confirmation chooses a model at its default level, without another menu.
    let mut terminal = Scripted::new(&[Some(0)]);
    let chosen = pick_model(&current, &mut terminal, &openai_only, &[], None).unwrap();
    assert_eq!(chosen.provider, Provider::openai());
    assert_eq!(chosen.model(), Provider::openai().models()[0].id);
    assert_eq!(chosen.selection.effort(), chosen.selection.model().default);
    assert_eq!(terminal.titles(), ["Model"]);
    let nobody = |_: &Provider| None;
    let mut terminal = Scripted::new(&[None]);
    pick_model(&current, &mut terminal, &nobody, &[], None);
    assert_eq!(terminal.offered("Model").len(), openrouter_models);
}

#[test]
fn each_providers_models_are_a_group_named_for_it() {
    let current = openrouter();
    let openai_too =
        |provider: &Provider| (*provider == Provider::openai()).then(|| "a".to_string());
    let mut terminal = Scripted::new(&[None]);
    pick_model(&current, &mut terminal, &openai_too, &[], None);
    let groups = vec![
        ("OpenAI".to_string(), Provider::openai().models().len()),
        (
            "OpenRouter".to_string(),
            Provider::openrouter().models().len(),
        ),
    ];
    assert_eq!(terminal.grouped, [groups]);
    // The group names the provider: a row's note is the model's id, which search finds.
    let (_, items) = &terminal.shown[0];
    assert_eq!(items[0].detail, Provider::openai().models()[0].id);
}

fn level(efforts: Vec<Effort>, at: Effort) -> Selection {
    let model = ModelEntry {
        id: "lab/model".into(),
        name: "Model".into(),
        default: at,
        efforts,
        context: 1000,
        output: 0,
        marks: false,
    };
    Selection::new(model).with_effort(at).unwrap()
}

#[test]
fn a_step_goes_a_level_up_or_down_and_says_when_there_is_none() {
    let levels = vec![Effort::Low, Effort::Medium, Effort::High];
    let up = stepped(&level(levels.clone(), Effort::Medium), true).unwrap();
    assert_eq!(up.effort(), Effort::High);
    let down = stepped(&level(levels.clone(), Effort::Medium), false).unwrap();
    assert_eq!(down.effort(), Effort::Low);
    assert_eq!(
        stepped(&level(levels, Effort::High), true),
        Err("lab/model takes no more: this is the most it reasons.\n".into())
    );
}

#[test]
fn a_model_with_no_levels_says_so_when_stepped() {
    for up in [true, false] {
        assert_eq!(
            stepped(&level(vec![], Effort::Off), up),
            Err("lab/model takes no reasoning levels.\n".into())
        );
    }
}

#[test]
fn a_models_levels_start_with_auto_which_is_where_a_new_model_starts() {
    let current = Choice {
        level: Level::Fixed,
        ..openrouter()
    };
    let openai_too =
        |provider: &Provider| (*provider == Provider::openai()).then(|| "a".to_string());
    let sol = Provider::openai()
        .models()
        .iter()
        .position(|m| m.id == "gpt-6-sol")
        .unwrap();
    let mut terminal = Scripted::new(&[Some(sol)]);
    let chosen = pick_model(&current, &mut terminal, &openai_too, &[], None).unwrap();
    let row = &terminal.adjusted[0][sol];
    assert_eq!((row.values[0].as_str(), row.at), ("auto", 0), "{row:?}");
    assert_eq!(chosen.level, Level::Auto);
    // A level after it is the person's.
    let mut terminal = Scripted::new(&[Some(sol)]);
    terminal.level = Some(2);
    let chosen = pick_model(&current, &mut terminal, &openai_too, &[], None).unwrap();
    let named = terminal.adjusted[0][sol].values[2].clone();
    assert_eq!(
        (chosen.level, chosen.selection.effort().name()),
        (Level::Fixed, named.as_str())
    );
}

#[test]
fn with_no_server_on_this_machine_the_list_says_which_are_looked_for() {
    let current = openrouter();
    let nobody = |_: &Provider| None;
    let mut terminal = Scripted::new(&[None]);
    let note = "none running (Ollama, LM Studio, llama-server)";
    pick_model(&current, &mut terminal, &nobody, &[], Some(note));
    let groups = &terminal.grouped[0];
    assert_eq!(groups.len(), 2, "{groups:?}");
    assert_eq!(groups[1], (note.to_string(), 0));
}
