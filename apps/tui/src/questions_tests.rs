use std::time::Instant;

use crossterm::event::KeyCode;
use protocol::{Asked, Choice as Offered, ToClient};

use super::*;
use crate::app::App;
use crate::style;
use crate::test_support::{key, live_area, replies, typed};

fn asked(choices: &[&str], multiple: bool) -> Question {
    Question {
        id: "q".into(),
        text: "Which checks?".into(),
        choices: choices
            .iter()
            .map(|label| Offered {
                label: label.to_string(),
                ..Offered::default()
            })
            .collect(),
        multiple,
        ..Question::default()
    }
}

fn press(app: &mut App, codes: &[KeyCode], now: Instant) {
    for code in codes {
        app.input(key(*code), now);
    }
}

#[test]
fn a_question_with_several_answers_is_checked_then_sent() {
    let now = Instant::now();
    let mut app = App::new(60, 20, vec![]);
    let question = asked(&["Tests", "Lint"], true);
    let ask = Asked::Question {
        question,
        at: 0,
        of: 2,
        headers: vec![],
    };
    app.asked(1, "", ask, now);
    press(
        &mut app,
        &[KeyCode::Char(' '), KeyCode::Down, KeyCode::Char(' ')],
        now,
    );
    insta::assert_snapshot!(live_area(&app, now), @"
    • Which checks? 1 of 2

      1. [✔] Tests
    ❯ 2. [✔] Lint
      3. Something else: type it

      space check · enter send · esc skip
    ");
    press(&mut app, &[KeyCode::Enter], now);
    let sent = answered(&mut app);
    assert_eq!(sent.choices, [0, 1]);
}

#[test]
fn an_open_question_is_a_line_to_type_at_once() {
    let now = Instant::now();
    let mut app = App::new(60, 20, vec![]);
    let ask = Asked::Question {
        question: asked(&[], false),
        at: 0,
        of: 1,
        headers: vec![],
    };
    app.asked(1, "", ask, now);
    typed(&mut app, "all of them", now);
    press(&mut app, &[KeyCode::Enter], now);
    let sent = answered(&mut app);
    assert_eq!(sent.note.as_deref(), Some("all of them"));
}

#[test]
fn a_choice_of_llx_starts_on_the_option_marked() {
    let now = Instant::now();
    let mut app = App::new(60, 20, vec![]);
    let options = vec![
        ("OpenRouter".to_string(), String::new()),
        ("OpenAI".to_string(), String::new()),
    ];
    let title = "Login › Provider".to_string();
    let choose = Asked::Choose {
        title,
        options,
        marked: Some(1),
    };
    app.asked(1, "", choose, now);
    insta::assert_snapshot!(live_area(&app, now), @"
    • Login › Provider

      1. OpenRouter
    ❯ 2. OpenAI

      enter select · esc cancel
    ");
    press(&mut app, &[KeyCode::Enter], now);
    assert_eq!(replies(&mut app), [(1, Reply::Choice(Some(1)))]);
}

#[test]
fn a_hidden_line_never_shows_the_secret() {
    let now = Instant::now();
    let mut app = App::new(60, 20, vec![]);
    let prompt = "Paste the key:".to_string();
    app.asked(
        1,
        "",
        Asked::Line {
            prompt,
            hidden: true,
        },
        now,
    );
    typed(&mut app, "sk-secret", now);
    let screen = live_area(&app, now);
    assert!(
        !screen.contains("sk-secret") && screen.contains("•••••••••"),
        "{screen}"
    );
    press(&mut app, &[KeyCode::Enter], now);
    let line = Reply::Line(Some("sk-secret".to_string()));
    assert_eq!(replies(&mut app), [(1, line)]);
}

#[test]
fn a_withdrawn_question_closes_unanswered_and_the_prompt_comes_back() {
    let now = Instant::now();
    let mut app = App::new(60, 20, vec![]);
    let ask = Asked::Question {
        question: asked(&["Tests"], false),
        at: 0,
        of: 1,
        headers: vec![],
    };
    app.asked(1, "", ask, now);
    app.frame(ToClient::Resolved { id: 1 }, now);
    assert_eq!(replies(&mut app), [], "nobody waits for a reply");
    assert_eq!(
        live_area(&app, now),
        format!(
            "{rule}\n❯ Type a task, / for commands\n{rule}\n  default mode · shift+tab to cycle",
            rule = "─".repeat(60)
        )
    );
}

#[test]
fn each_pick_sends_what_it_means() {
    let approval = answer(&Open::Approval(1), Picked::Option(1));
    assert_eq!(
        approval,
        (Some((1, Reply::Approval(Approval::Always))), None)
    );
    let said = answer(&Open::Approval(2), Picked::Typed(2, String::new()));
    let no = Reply::Approval(Approval::No(None));
    assert_eq!(said, (Some((2, no)), Some(String::new())));
    let typed = answer(&Open::Question(3), Picked::Typed(0, String::new()));
    let empty = Reply::Question(Some(Answer::default()));
    assert_eq!(typed, (Some((3, empty)), None));
    let skipped = answer(&Open::Question(4), Picked::Cancelled);
    assert_eq!(skipped, (Some((4, Reply::Question(None))), None));
    let cancelled = answer(&Open::Choice(5), Picked::Cancelled);
    assert_eq!(cancelled, (Some((5, Reply::Choice(None))), None));
    assert_eq!(answer(&Open::Quit, Picked::Option(0)), (None, None));
}

/// The one answer the person gave to question 1.
fn answered(app: &mut App) -> Answer {
    match &replies(app)[..] {
        [(1, Reply::Question(Some(answer)))] => answer.clone(),
        other => panic!("not an answer: {other:?}"),
    }
}

#[test]
fn a_steps_line_is_headed_by_its_path_and_says_what_to_do_under_it() {
    let select = line(
        "Login › OpenAI › API key: paste it and press Enter (it is not shown)",
        true,
    );
    let shown: Vec<String> = select.render(60).0.iter().map(style::plain).collect();
    assert_eq!(shown[0], "• Login › OpenAI › API key");
    assert_eq!(shown[1], "  paste it and press Enter (it is not shown)");
}

/// "Which layout?" with two options, each with a mockup, under the header `Layout`.
fn layouts() -> Asked {
    let option = |label: &str, description: &str, preview: &str| Offered {
        label: label.into(),
        description: description.into(),
        preview: preview.into(),
    };
    let question = Question {
        id: "layout".into(),
        header: "Layout".into(),
        text: "Which layout?".into(),
        choices: vec![
            option(
                "Sidebar",
                "Nav on the left",
                "+--+-----+\n|  |     |\n+--+-----+",
            ),
            option("Top nav", "", "+--------+\n|  menu  |\n+--------+"),
        ],
        multiple: false,
    };
    Asked::Question {
        question,
        at: 0,
        of: 1,
        headers: vec!["Layout".into()],
    }
}

#[test]
fn the_marked_options_preview_is_shown_beside_the_options() {
    let now = Instant::now();
    let mut app = App::new(60, 30, vec![]);
    app.asked(1, "", layouts(), now);
    insta::assert_snapshot!(live_area(&app, now), @"
    ☐ Layout
    • Which layout?

    ❯ 1. Sidebar                  ┌────────────┐
      2. Top nav                  │ +--+-----+ │
      3. Something else: type it  │ |  |     | │
                                  │ +--+-----+ │
                                  └────────────┘
                                  Nav on the left

      enter select · esc skip
    ");
    press(&mut app, &[KeyCode::Down], now);
    insta::assert_snapshot!(live_area(&app, now), @"
    ☐ Layout
    • Which layout?

      1. Sidebar                  ┌────────────┐
    ❯ 2. Top nav                  │ +--------+ │
      3. Something else: type it  │ |  menu  | │
                                  │ +--------+ │
                                  └────────────┘

      enter select · esc skip
    ");
    press(&mut app, &[KeyCode::Enter], now);
    assert_eq!(answered(&mut app).choices, [1]);
}

#[test]
fn on_a_narrow_terminal_the_preview_goes_under_the_options() {
    let now = Instant::now();
    let mut app = App::new(36, 30, vec![]);
    app.asked(1, "", layouts(), now);
    insta::assert_snapshot!(live_area(&app, now), @"
    ☐ Layout
    • Which layout?

    ❯ 1. Sidebar
      2. Top nav
      3. Something else: type it

      ┌────────────┐
      │ +--+-----+ │
      │ |  |     | │
      │ +--+-----+ │
      └────────────┘
      Nav on the left

      enter select · esc skip
    ");
}

#[test]
fn the_headers_are_tabs_that_say_which_questions_are_answered() {
    let now = Instant::now();
    let mut app = App::new(60, 30, vec![]);
    let mut question = asked(&["Search", "Dark mode"], true);
    question.header = "Features".into();
    question.text = "Which features?".into();
    let ask = Asked::Question {
        question,
        at: 1,
        of: 3,
        headers: vec!["Layout".into(), "Features".into(), "Name".into()],
    };
    app.asked(1, "", ask, now);
    insta::assert_snapshot!(live_area(&app, now), @"
    ☒ Layout  ☐ Features  ☐ Name
    • Which features?

    ❯ 1. [ ] Search
      2. [ ] Dark mode
      3. Something else: type it

      space check · enter send · esc skip
    ");
}

#[test]
fn a_preview_cannot_redraw_the_screen_and_a_long_one_is_cut() {
    let now = Instant::now();
    let mut app = App::new(60, 40, vec![]);
    let Asked::Question {
        mut question,
        headers,
        ..
    } = layouts()
    else {
        panic!("a question")
    };
    let rows: Vec<String> = (1..=30).map(|n| format!("row {n}\x1b[2J")).collect();
    question.choices[0].preview = rows.join("\n");
    let ask = Asked::Question {
        question,
        at: 0,
        of: 1,
        headers,
    };
    app.asked(1, "", ask, now);
    let screen = live_area(&app, now);
    assert!(!screen.contains('\x1b'), "{screen}");
    assert!(
        screen.contains("row 19") && !screen.contains("row 20"),
        "{screen}"
    );
    assert!(screen.contains("11 more lines"), "{screen}");
}
