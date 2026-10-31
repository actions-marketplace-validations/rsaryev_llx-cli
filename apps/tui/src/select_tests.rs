use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::text::{Line, Span};

use super::*;
use crate::style;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn approval() -> Select {
    Select::new(
        vec![
            Line::from("Run this command?"),
            Line::from("  cargo test --workspace"),
        ],
        vec![
            Choice::new("Yes"),
            Choice::new("Yes, and don't ask again for bash"),
            Choice::new("No, and tell llx what to do differently").types(),
        ],
        "esc no",
    )
}

fn shown(select: &Select, width: usize) -> String {
    select
        .render(width)
        .0
        .iter()
        .map(style::plain)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn an_approval_reads_as_a_short_list() {
    insta::assert_snapshot!(shown(&approval(), 60), @"
    Run this command?
      cargo test --workspace

    ❯ 1. Yes
      2. Yes, and don't ask again for bash
      3. No, and tell llx what to do differently

      enter select · esc no
    ");
}

#[test]
fn arrows_and_numbers_pick() {
    let mut select = approval();
    assert_eq!(select.key(key(KeyCode::Down)), None);
    assert_eq!(select.key(key(KeyCode::Enter)), Some(Picked::Option(1)));
    assert_eq!(
        approval().key(key(KeyCode::Char('1'))),
        Some(Picked::Option(0))
    );
    assert_eq!(approval().key(key(KeyCode::Char('y'))), None);
    assert_eq!(approval().key(key(KeyCode::Char('9'))), None);
    assert_eq!(approval().key(key(KeyCode::Esc)), Some(Picked::Cancelled));
}

#[test]
fn a_number_picks_nothing_when_there_are_no_options() {
    let mut select = Select::new(vec![Line::from("Pick one")], vec![], "esc cancel");
    assert_eq!(select.key(key(KeyCode::Char('1'))), None);
}

#[test]
fn an_option_that_types_opens_a_line_and_esc_goes_back() {
    let mut select = approval();
    assert_eq!(select.key(key(KeyCode::Char('3'))), None);
    for c in "use make".chars() {
        select.key(key(KeyCode::Char(c)));
    }
    let (rows, cursor) = select.render(60);
    assert_eq!(style::plain(&rows[cursor.unwrap().0]), "  ❯ use make");
    assert_eq!(cursor.unwrap().1, 12);
    assert_eq!(
        select.key(key(KeyCode::Enter)),
        Some(Picked::Typed(2, "use make".into()))
    );
    let mut select = approval();
    select.key(key(KeyCode::Char('3')));
    assert_eq!(select.key(key(KeyCode::Esc)), None);
    assert_eq!(select.key(key(KeyCode::Esc)), Some(Picked::Cancelled));
}

#[test]
fn a_long_option_wraps_under_its_label() {
    let select = Select::new(
        vec![],
        vec![Choice::new("A choice whose label is long").detail("and its detail too")],
        "esc skip",
    );
    insta::assert_snapshot!(shown(&select, 30), @"
    ❯ 1. A choice whose label is
         long and its detail too

      enter select · esc skip
    ");
}

#[test]
fn a_line_alone_shows_no_list_and_esc_cancels() {
    let mut select = Select::new(
        vec![Line::from("Paste the key")],
        vec![Choice::new("Type it").types()],
        "esc cancel",
    )
    .hidden(true)
    .typing(0);
    for c in "sk-1".chars() {
        select.key(key(KeyCode::Char(c)));
    }
    insta::assert_snapshot!(shown(&select, 40), @"
    Paste the key

      ❯ ••••

      enter send · esc cancel
    ");
    assert_eq!(select.key(key(KeyCode::Esc)), Some(Picked::Cancelled));
}

#[test]
fn several_options_are_checked_with_space_and_sent_with_enter() {
    let choices = vec![
        Choice::new("Tests"),
        Choice::new("Docs"),
        Choice::new("Lint"),
        Choice::new("Other").types(),
    ];
    let mut select = Select::new(vec![], choices, "esc skip").multiple(true);
    select.key(key(KeyCode::Char(' ')));
    select.key(key(KeyCode::Down));
    select.key(key(KeyCode::Down));
    select.key(key(KeyCode::Char(' ')));
    insta::assert_snapshot!(shown(&select, 60), @"
      1. [✔] Tests
      2. [ ] Docs
    ❯ 3. [✔] Lint
      4. Other

      space check · enter send · esc skip
    ");
    assert_eq!(
        select.key(key(KeyCode::Enter)),
        Some(Picked::Many(vec![0, 2]))
    );
}

#[test]
fn a_long_list_shows_a_window_around_the_marked_option() {
    let choices: Vec<Choice> = (0..40)
        .map(|n| Choice::new(&format!("model {n}")))
        .collect();
    let mut select = Select::new(vec![Line::from("Model")], choices, "↑↓ to choose");
    for _ in 0..20 {
        select.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    let (rows, _) = select.render(60);
    let text: Vec<String> = rows.iter().map(style::plain).collect();
    assert!(text.iter().any(|row| row == "  ↑ 14 more"), "{text:?}");
    assert!(
        text.iter().any(|row| row.starts_with("❯ model 20")),
        "{text:?}"
    );
    assert!(text.iter().any(|row| row == "  ↓ 14 more"), "{text:?}");
    assert!(!text.iter().any(|row| row.contains("model 0")));
}

#[test]
fn letters_typed_ahead_into_an_approval_pick_nothing() {
    // Typing a message as the approval opens must not approve ("a" was "don't ask again").
    let mut select = crate::questions::approval("bash", "echo second", None);
    for c in "also n y".chars() {
        assert_eq!(select.key(key(KeyCode::Char(c))), None, "{c:?} picked");
    }
    assert_eq!(select.key(key(KeyCode::Enter)), Some(Picked::Option(0)));
}

#[test]
fn a_heading_too_long_for_a_row_goes_on_under_its_words() {
    let title = vec![Line::from(vec![
        Span::raw("• "),
        Span::raw("Login › OpenAI › a step whose name is long"),
    ])];
    let select = Select::new(title, vec![Choice::new("Yes")], "esc cancel");
    insta::assert_snapshot!(shown(&select, 30), @"
    • Login › OpenAI › a step
      whose name is long

    ❯ 1. Yes

      enter select · esc cancel
    ");
}
