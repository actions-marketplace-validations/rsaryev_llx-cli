use super::*;

fn entry(efforts: &[Effort], default: Effort) -> ModelEntry {
    ModelEntry {
        context: 0,
        output: 0,
        marks: false,
        id: "m".into(),
        name: "M".into(),
        efforts: efforts.to_vec(),
        default,
    }
}

#[test]
fn a_selection_starts_at_an_offered_level() {
    use Effort::*;
    // The levels a model takes, its default, the level a selection starts at, what it
    // offers, and a level it refuses.
    type Case = (&'static [Effort], Effort, Effort, &'static [Effort], Effort);
    let cases: [Case; 3] = [
        (
            &[Low, Medium, High],
            Medium,
            Medium,
            &[Low, Medium, High],
            Max,
        ),
        // A default the model does not offer becomes its lowest level.
        (&[Low, High], Ultra, Low, &[Low, High], Ultra),
        // A model with no levels runs with none.
        (&[], High, Off, &[Off], Low),
    ];
    for (efforts, default, starts, offered, refused) in cases {
        let selection = Selection::new(entry(efforts, default));
        assert_eq!(selection.effort(), starts, "{efforts:?}");
        assert_eq!(selection.model().offered(), offered, "{efforts:?}");
        assert!(selection.with_effort(refused).is_none(), "{efforts:?}");
    }
}

#[test]
fn only_an_offered_level_can_be_chosen() {
    let selection = Selection::new(entry(&[Effort::Low, Effort::High], Effort::Low));
    assert_eq!(
        selection.with_effort(Effort::High).map(|s| s.effort()),
        Some(Effort::High)
    );
    assert_eq!(selection.with_effort(Effort::Max), None);
    assert_eq!(selection.with_effort(Effort::Off), None);
}

#[test]
fn levels_read_by_name_and_none_is_off() {
    assert_eq!(Effort::named(" XHigh "), Some(Effort::XHigh));
    assert_eq!(Effort::named("none"), Some(Effort::Off));
    assert_eq!(Effort::named("turbo"), None);
    for effort in Effort::ALL {
        assert_eq!(Effort::named(effort.name()), Some(effort));
    }
}

#[test]
fn a_conversations_own_level_stands_where_the_model_offers_it() {
    let model = ModelEntry {
        id: "lab/model".into(),
        name: "Model".into(),
        efforts: vec![Effort::Low, Effort::High],
        default: Effort::High,
        context: 1000,
        output: 0,
        marks: false,
    };
    let chosen = Selection::new(model);
    assert_eq!(chosen.or_at(Some(Effort::Low)).effort(), Effort::Low);
    assert_eq!(chosen.or_at(Some(Effort::Medium)).effort(), Effort::High);
    assert_eq!(chosen.or_at(None), chosen);
}
