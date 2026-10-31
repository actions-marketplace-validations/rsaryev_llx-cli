use std::time::Instant;

use crossterm::event::KeyCode;
use protocol::{Adjustment, Asked, Group, Reply};

use crate::app::App;
use crate::test_support::{Term, key, replies, typed};

/// `/model` as the server asks it: three providers' models, each a group, marked on `marked`.
fn models(marked: usize) -> App {
    let mut rows: Vec<(String, String)> = [
        ("GPT-6.1 Sol", "gpt-6.1-sol"),
        ("GPT-6 Luna", "gpt-6-luna"),
        ("GPT-5", "gpt-5"),
    ]
    .iter()
    .map(|(name, id)| (name.to_string(), id.to_string()))
    .collect();
    rows.extend((1..=15).map(|n| (format!("Router {n:02}"), format!("router/model-{n:02}"))));
    rows.extend([
        ("Claude Opus".to_string(), "claude-opus".to_string()),
        ("Claude Haiku".to_string(), "claude-haiku".to_string()),
    ]);
    let level = Adjustment {
        values: vec!["low".into(), "high".into()],
        at: 0,
    };
    let group = |name: &str, rows| Group {
        name: name.into(),
        rows,
    };
    let asked = Asked::ChooseAdjustable {
        title: "Model".into(),
        adjustments: vec![level; rows.len()],
        options: rows,
        marked: Some(marked),
        groups: vec![
            group("OpenAI", 3),
            group("OpenRouter", 15),
            group("Anthropic", 2),
        ],
    };
    let mut app = App::new(80, 40, vec![]);
    app.asked(1, "", asked, Instant::now());
    app
}

fn press(app: &mut App, codes: &[KeyCode]) {
    for code in codes {
        app.input(key(*code), Instant::now());
    }
}

fn screen(app: &mut App) -> String {
    Term::new(app).draw(app, Instant::now())
}

fn picked(app: &mut App) -> Vec<(u64, Reply)> {
    replies(app)
}

#[test]
fn a_grouped_list_opens_on_the_marked_rows_tab_with_its_rows_alone() {
    let mut app = models(5);
    let shown = screen(&mut app);
    assert!(
        shown.contains("OpenAI 3 · [OpenRouter 15] · Anthropic 2"),
        "{shown}"
    );
    assert!(shown.contains("❯ Router 03"), "{shown}");
    assert!(
        !shown.contains("GPT-6.1 Sol"),
        "another tab's rows: {shown}"
    );
    assert!(
        !shown.contains("Claude Opus"),
        "another tab's rows: {shown}"
    );
    assert!(shown.contains("tab provider · ←→ reasoning"), "{shown}");
}

#[test]
fn tab_and_shift_tab_go_round_the_tabs_and_enter_picks_in_the_whole_list() {
    let mut app = models(5);
    press(&mut app, &[KeyCode::Tab]);
    let shown = screen(&mut app);
    assert!(shown.contains("[Anthropic 2]"), "{shown}");
    assert!(shown.contains("❯ Claude Opus"), "{shown}");
    assert!(!shown.contains("Router 03"), "{shown}");
    // Past the last tab, the first; Shift+Tab back.
    press(&mut app, &[KeyCode::Tab]);
    assert!(screen(&mut app).contains("[OpenAI 3]"));
    press(&mut app, &[KeyCode::BackTab]);
    assert!(screen(&mut app).contains("[Anthropic 2]"));
    press(&mut app, &[KeyCode::Down, KeyCode::Right, KeyCode::Enter]);
    assert_eq!(picked(&mut app), [(1, Reply::Adjusted(Some((19, 1))))]);
}

#[test]
fn up_and_down_stay_in_the_tab_and_each_tab_keeps_its_marker() {
    let mut app = models(5);
    press(&mut app, &[KeyCode::End]);
    assert!(screen(&mut app).contains("❯ Router 15"));
    press(&mut app, &[KeyCode::Down, KeyCode::Tab, KeyCode::BackTab]);
    let shown = screen(&mut app);
    assert!(shown.contains("❯ Router 15"), "{shown}");
    press(&mut app, &[KeyCode::Enter]);
    assert_eq!(picked(&mut app), [(1, Reply::Adjusted(Some((17, 0))))]);
}

#[test]
fn a_search_looks_in_every_tab_and_goes_to_the_one_that_matches() {
    let mut app = models(5);
    typed(&mut app, "luna", Instant::now());
    let shown = screen(&mut app);
    assert!(
        shown.contains("[OpenAI 1] · OpenRouter 0 · Anthropic 0"),
        "{shown}"
    );
    assert!(shown.contains("❯ GPT-6 Luna"), "{shown}");
    press(&mut app, &[KeyCode::Enter]);
    assert_eq!(picked(&mut app), [(1, Reply::Adjusted(Some((1, 0))))]);
}

#[test]
fn a_search_stays_on_its_tab_while_the_tab_has_a_match() {
    let mut app = models(0);
    // "claude" matches in Anthropic only; "gpt" in OpenAI, the tab shown.
    typed(&mut app, "gpt", Instant::now());
    let shown = screen(&mut app);
    assert!(shown.contains("[OpenAI 3]"), "{shown}");
    press(&mut app, &[KeyCode::Tab]);
    let shown = screen(&mut app);
    assert!(shown.contains("[OpenRouter 0]"), "{shown}");
    assert!(shown.contains("No model matches 'gpt'"), "{shown}");
}

#[test]
fn one_providers_list_is_named_by_its_tab_with_nothing_to_switch() {
    let asked = Asked::ChooseAdjustable {
        title: "Model".into(),
        options: vec![("GPT-6 Luna".into(), "gpt-6-luna".into())],
        marked: Some(0),
        adjustments: vec![],
        groups: vec![Group {
            name: "OpenAI".into(),
            rows: 1,
        }],
    };
    let mut app = App::new(80, 40, vec![]);
    app.asked(1, "", asked, Instant::now());
    let shown = screen(&mut app);
    assert!(shown.contains("[OpenAI 1]"), "{shown}");
    assert!(!shown.contains("tab provider"), "{shown}");
}
