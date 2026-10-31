use std::time::Instant;

use crossterm::event::KeyCode;

use super::*;
use crate::app::App;
use protocol::{Asked, Reply};

use crate::test_support::{Term, ctrl, key, replies, typed};

/// A long list as `/login` shows it: a name, then the id and a note.
fn providers() -> Vec<(String, String)> {
    let mut named: Vec<(String, String)> = [
        ("OpenAI", "openai"),
        ("Anthropic", "anthropic"),
        ("OpenRouter", "openrouter"),
        ("DeepSeek", "deepseek"),
        ("Hyperopen", "hyperopen"),
    ]
    .iter()
    .map(|(name, id)| (name.to_string(), format!("{id} · not signed in")))
    .collect();
    named.extend((1..=25).map(|n| (format!("Vendor {n:02}"), format!("vendor-{n:02} · signed"))));
    named
}

/// `options` put to the person under `title`, as question 1.
fn choose(title: &str, options: Vec<(String, String)>) -> App {
    let mut app = App::new(80, 40, vec![]);
    let asked = Asked::Choose {
        title: title.into(),
        options,
        marked: None,
    };
    app.asked(1, "", asked, Instant::now());
    app
}

/// The pick replied, if one was.
#[expect(
    clippy::option_option,
    reason = "no reply yet, or the reply: a pick or a cancel"
)]
fn picked(app: &mut App) -> Option<Option<usize>> {
    match &replies(app)[..] {
        [] => None,
        [(1, Reply::Choice(picked))] => Some(*picked),
        other => panic!("not a pick: {other:?}"),
    }
}

/// The row and value replied, if they were.
#[expect(
    clippy::option_option,
    reason = "no reply yet, or the reply: a pick or a cancel"
)]
fn adjusted(app: &mut App) -> Option<Option<(usize, usize)>> {
    match &replies(app)[..] {
        [] => None,
        [(1, Reply::Adjusted(picked))] => Some(*picked),
        other => panic!("not a pick: {other:?}"),
    }
}

fn press(app: &mut App, codes: &[KeyCode]) {
    for code in codes {
        app.input(key(*code), Instant::now());
    }
}

fn screen(app: &mut App) -> String {
    Term::new(app).draw(app, Instant::now())
}

fn adjustable(
    options: Vec<(String, String)>,
    adjustments: Vec<protocol::Adjustment>,
    marked: Option<usize>,
) -> App {
    let mut app = App::new(80, 40, vec![]);
    let asked = Asked::ChooseAdjustable {
        title: "Model".into(),
        options,
        marked,
        adjustments,
        groups: vec![],
    };
    app.asked(1, "", asked, Instant::now());
    app
}

fn levels(values: &[&str], at: usize) -> protocol::Adjustment {
    protocol::Adjustment {
        values: values.iter().map(|value| value.to_string()).collect(),
        at,
    }
}

#[test]
fn adjustable_rows_bound_arrows_and_remember_each_rows_value() {
    let mut app = adjustable(
        providers()[..2].to_vec(),
        vec![
            levels(&["low", "medium", "high"], 1),
            levels(&["off", "on"], 0),
        ],
        Some(0),
    );
    let shown = screen(&mut app);
    assert!(shown.contains("← medium →"), "{shown}");
    assert!(shown.contains("←→ reasoning"), "{shown}");
    press(
        &mut app,
        &[
            KeyCode::Right,
            KeyCode::Right,
            KeyCode::Down,
            KeyCode::Right,
            KeyCode::Up,
        ],
    );
    let shown = screen(&mut app);
    assert!(shown.contains("← high →"), "{shown}");
    // Off the marked row, its value without arrows: only the marked one adjusts.
    let other = shown
        .lines()
        .find(|line| line.contains("Anthropic"))
        .unwrap();
    assert!(other.trim_end().ends_with(" on"), "{shown}");
    assert!(!other.contains('←'), "{shown}");
    press(
        &mut app,
        &[KeyCode::Left, KeyCode::Left, KeyCode::Left, KeyCode::Enter],
    );
    assert_eq!(adjusted(&mut app), Some(Some((0, 0))));
}

#[test]
fn adjustable_search_keeps_values_on_the_original_rows() {
    let options = providers();
    let adjustments = options
        .iter()
        .map(|_| levels(&["low", "high"], 0))
        .collect();
    let mut app = adjustable(options, adjustments, Some(3));
    press(&mut app, &[KeyCode::Right]);
    typed(&mut app, "openai", Instant::now());
    press(&mut app, &[KeyCode::Right, KeyCode::Esc]);
    typed(&mut app, "deepseek", Instant::now());
    let shown = screen(&mut app);
    assert!(shown.contains("❯ DeepSeek"), "{shown}");
    assert!(shown.contains("← high →"), "{shown}");
    press(&mut app, &[KeyCode::Enter]);
    assert_eq!(adjusted(&mut app), Some(Some((3, 1))));
}

#[test]
fn adjustable_empty_single_and_missing_values_are_harmless() {
    let options = providers()[..3].to_vec();
    for row in 0..3 {
        let mut app = adjustable(
            options.clone(),
            vec![levels(&[], 9), levels(&["fixed"], 9)],
            Some(row),
        );
        press(&mut app, &[KeyCode::Left, KeyCode::Right, KeyCode::Enter]);
        assert_eq!(adjusted(&mut app), Some(Some((row, 0))));
    }
    let mut app = adjustable(vec![], vec![], None);
    press(&mut app, &[KeyCode::Right, KeyCode::Left, KeyCode::Enter]);
    assert!(adjusted(&mut app).is_none());
    press(&mut app, &[KeyCode::Esc]);
    assert_eq!(adjusted(&mut app), Some(None));
}

#[test]
fn adjustable_cancel_and_no_matches_do_not_choose_a_row() {
    let options = providers();
    let adjustments = options
        .iter()
        .map(|_| levels(&["low", "high"], 0))
        .collect();
    let mut app = adjustable(options, adjustments, None);
    typed(&mut app, "xyz", Instant::now());
    press(&mut app, &[KeyCode::Right, KeyCode::Enter]);
    assert!(adjusted(&mut app).is_none());
    press(&mut app, &[KeyCode::Esc, KeyCode::Esc]);
    assert_eq!(adjusted(&mut app), Some(None));
}

#[test]
fn a_long_list_scrolls_to_its_end_and_back() {
    let mut app = choose("Provider", providers());
    for _ in 0..29 {
        press(&mut app, &[KeyCode::Down]);
    }
    let shown = screen(&mut app);
    assert!(shown.contains("❯ Vendor 25"), "{shown}");
    assert!(shown.contains("↑ 18 more"), "{shown}");
    assert_eq!(shown.matches(" more").count(), 1, "nothing below:\n{shown}");
    press(&mut app, &[KeyCode::Home]);
    let shown = screen(&mut app);
    assert!(shown.contains("❯ OpenAI"), "{shown}");
    assert!(shown.contains("↓ 18 more"), "{shown}");
    press(&mut app, &[KeyCode::End]);
    assert!(screen(&mut app).contains("❯ Vendor 25"));
    press(&mut app, &[KeyCode::Home, KeyCode::PageDown]);
    assert!(
        screen(&mut app).contains("❯ Vendor 08"),
        "a page is 12 rows"
    );
    press(&mut app, &[KeyCode::PageUp]);
    assert!(screen(&mut app).contains("❯ OpenAI"));
}

#[test]
fn typing_filters_the_list_by_name_and_id_prefix_first() {
    let mut app = choose("Provider", providers());
    typed(&mut app, "OPEN", Instant::now());
    insta::assert_snapshot!(screen(&mut app), @"
    • Provider

      Search: OPEN▏  3 matches

    ❯ OpenAI      openai · not signed in
      OpenRouter  openrouter · not signed in
      Hyperopen   hyperopen · not signed in

      enter select · esc clear
    ");
}

#[test]
fn backspace_edits_the_query_and_esc_clears_it_then_cancels() {
    let mut app = choose("Provider", providers());
    typed(&mut app, "deepx", Instant::now());
    assert!(screen(&mut app).contains("No provider matches 'deepx'"));
    press(&mut app, &[KeyCode::Backspace]);
    let shown = screen(&mut app);
    assert!(shown.contains("Search: deep▏  1 match"), "{shown}");
    assert!(shown.contains("❯ DeepSeek"), "{shown}");
    press(&mut app, &[KeyCode::Esc]);
    let shown = screen(&mut app);
    assert!(
        shown.contains("Search: ▏"),
        "the query is cleared:\n{shown}"
    );
    assert!(shown.contains("OpenAI"), "every row is back:\n{shown}");
    assert!(picked(&mut app).is_none(), "the first Esc does not cancel");
    press(&mut app, &[KeyCode::Esc]);
    assert_eq!(picked(&mut app), Some(None));
}

#[test]
fn enter_picks_the_row_marked_among_the_matches() {
    let mut app = choose("Provider", providers());
    // Digits are part of the query here, not picks.
    typed(&mut app, "vendor-1", Instant::now());
    press(&mut app, &[KeyCode::Down, KeyCode::Enter]);
    assert_eq!(
        picked(&mut app),
        Some(Some(15)),
        "Vendor 11, the second match"
    );
}

#[test]
fn nothing_matching_enter_picks_nothing() {
    let mut app = choose("Model (DeepSeek)", providers());
    typed(&mut app, "xyz", Instant::now());
    assert!(screen(&mut app).contains("No model matches 'xyz'"));
    press(&mut app, &[KeyCode::Enter]);
    assert!(picked(&mut app).is_none());
}

#[test]
fn a_short_list_is_numbered_and_takes_no_query() {
    let options = vec![
        ("Keep it".to_string(), String::new()),
        ("Sign in again".to_string(), String::new()),
    ];
    let mut app = choose("Signed in", options);
    typed(&mut app, "s", Instant::now());
    let shown = screen(&mut app);
    assert!(!shown.contains("Search"), "{shown}");
    assert!(shown.contains("❯ 1. Keep it"), "{shown}");
    press(&mut app, &[KeyCode::Char('2')]);
    assert_eq!(picked(&mut app), Some(Some(1)));
}

#[test]
fn prefix_matches_come_first_then_substrings_then_word_starts() {
    let rows: Vec<[String; 2]> = ["Google AI Studio", "Vegas", "Gasline", "Other"]
        .iter()
        .map(|name| keys(name, ""))
        .collect();
    assert_eq!(ranked("GAS", &rows), [2, 1, 0]);
    assert_eq!(ranked("", &rows), [0, 1, 2, 3]);
}

#[test]
fn a_row_is_found_by_its_label_and_the_lead_of_its_detail_only() {
    let row = [keys("DeepSeek", "deepseek · not signed in")];
    assert_eq!(ranked("deeps", &row), [0]);
    assert!(ranked("signed", &row).is_empty(), "a note is not searched");
}

#[test]
fn ctrl_c_clears_a_query_before_it_closes_the_list() {
    let mut app = choose("Provider", providers());
    typed(&mut app, "open", Instant::now());
    app.input(ctrl('c'), Instant::now());
    assert_eq!(picked(&mut app), None, "the query went, not the list");
    let shown = screen(&mut app);
    assert!(shown.contains("30 in all"), "{shown}");
    app.input(ctrl('c'), Instant::now());
    assert_eq!(picked(&mut app), Some(None));
}

#[test]
fn ctrl_u_clears_the_query() {
    let mut app = choose("Provider", providers());
    typed(&mut app, "open", Instant::now());
    app.input(ctrl('u'), Instant::now());
    let shown = screen(&mut app);
    assert!(shown.contains("Search: ▏  30 in all"), "{shown}");
    assert_eq!(picked(&mut app), None);
}

#[test]
fn a_list_in_steps_says_what_it_holds_by_its_last_step() {
    let mut app = choose("Login › Provider", providers());
    typed(&mut app, "zzz", Instant::now());
    let shown = screen(&mut app);
    assert!(shown.contains("No provider matches 'zzz'"), "{shown}");
}

#[test]
fn a_models_levels_line_up_even_when_an_id_is_long() {
    let options = [
        ("Claude Haiku 5.5", "anthropic/claude-haiku-5.5"),
        ("Mistral Large 4", "mistralai/mistral-large-4-0"),
        ("Gemini 4 Argon", "google/gemini-4-argon"),
    ];
    let options = options
        .iter()
        .map(|(name, id)| (name.to_string(), id.to_string()))
        .collect();
    let levels = vec![
        levels(&["off", "low"], 0),
        levels(&["off", "medium"], 1),
        levels(&["off", "high"], 1),
    ];
    let mut app = adjustable(options, levels, Some(0));
    app.input(key(KeyCode::Down), Instant::now());
    let shown = screen(&mut app);
    // Where each row's level starts: the same column, the marked row's arrows around it.
    let column = |name: &str, level: &str| {
        let line = shown.lines().find(|line| line.contains(name)).unwrap();
        line.split(level).next().unwrap().chars().count()
    };
    assert_eq!(column("Claude", "off"), column("Gemini", "high"), "{shown}");
    assert_eq!(
        column("Claude", "off"),
        column("Mistral", "medium"),
        "{shown}"
    );
}
