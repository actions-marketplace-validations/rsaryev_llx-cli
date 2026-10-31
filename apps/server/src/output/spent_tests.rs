use agent::observer::Observer;
use contracts::chat::Usage;
use contracts::history::Step;
use contracts::history::Turn;
use protocol::{Event, Total};

use super::*;
use crate::testing::{events, fake_ui, quiet};

fn spent(input_tokens: u64, cached_tokens: u64, output_tokens: u64, cost: f64) -> Spent {
    Spent {
        input_tokens,
        cached_tokens,
        output_tokens,
        cost,
    }
}

/// The totals the interface was told, in order.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
fn totals(frames: &std::sync::mpsc::Receiver<protocol::ToClient>) -> Vec<Total> {
    let told = events(frames).into_iter();
    told.filter_map(|(_, event)| match event {
        Event::Total(total) => Some(total),
        _ => None,
    })
    .collect()
}

#[test]
fn a_resumed_session_begins_from_what_its_tasks_spent_and_grows_with_each_turn() {
    let (ui, frames) = fake_ui();
    let output = quiet().in_ui(Some(ui.for_lane("s1")));
    let mut session = Session::default();
    let step = |what: &str, spent| Step {
        what: what.into(),
        spent,
        ..Step::default()
    };
    session.turns.push(Turn {
        goal: "fix it".into(),
        answer: String::new(),
        status: "done".into(),
        at: 0,
        steps: vec![
            step("model", spent(10_000, 8_000, 200, 0.01)),
            step("bash", Spent::default()),
            step("decision", spent(500, 0, 20, 0.002)),
        ],
    });
    output.spent_before(&session);
    output.turn(&Usage {
        input_tokens: 1_000,
        cached_tokens: 900,
        output_tokens: 50,
        cost: 0.001,
        ..Usage::default()
    });
    let told = totals(&frames);
    assert_eq!(told.len(), 2, "{told:?}");
    assert_eq!(
        (
            told[0].input_tokens,
            told[0].cached_tokens,
            told[0].output_tokens
        ),
        (10_500, 8_000, 220)
    );
    assert!((told[0].cost - 0.012).abs() < 1e-9);
    assert_eq!(told[1].input_tokens, 11_500);
    assert_eq!(told[1].cached_tokens, 8_900);
    assert!((told[1].cost - 0.013).abs() < 1e-9);
}

#[test]
fn a_new_session_says_nothing_until_it_spends() {
    let (ui, frames) = fake_ui();
    let output = quiet().in_ui(Some(ui.for_lane("s1")));
    output.spent_before(&Session::default());
    assert!(totals(&frames).is_empty());
}
