use std::time::Instant;

use crossterm::event::KeyCode;
use ratatui::text::Span;

use super::*;
use crate::style;
use crate::test_support::{call, in_session, label, press, ran, render as screen};
use protocol::Event;

fn rows(app: &App) -> Vec<String> {
    let spinner = |_| Span::raw("*");
    render(app, Instant::now(), &spinner)
        .iter()
        .map(style::plain)
        .collect()
}

fn started(app: &mut App, id: &str, name: &str) {
    let now = Instant::now();
    app.event(
        Event::Started {
            id: id.into(),
            name: name.into(),
        },
        now,
    );
}

#[test]
fn no_agents_no_tree() {
    assert!(rows(&App::new(60, 20, vec![])).is_empty());
}

#[test]
fn each_session_is_a_row_the_shown_one_marked() {
    let now = Instant::now();
    let mut app = App::new(60, 20, vec![]);
    app.event(Event::TaskStarted("go".into()), now);
    let send = label("Messaging", "Messaged", "agent tests · run them");
    app.event(call("c1", "agents", &send), now);
    started(&mut app, "s1", "scout");
    started(&mut app, "s2", "tests");
    let agent = |event| in_session("s2", event);
    app.frame(agent(Event::TaskStarted("run them".into())), now);
    app.frame(agent(call("c2", "bash", &ran("cargo test"))), now);
    let ended = Event::TaskEnded {
        status: "done".into(),
        millis: 1,
        turns: 1,
        cost: 0.0,
    };
    app.frame(in_session("s1", ended), now);
    assert_eq!(
        rows(&app),
        [
            "  * go  Messaging agent tests · run them",
            "    ├ ✓ scout  done",
            "    └ * tests  Running cargo test",
            "  ↓ manage · ← sessions",
        ]
    );
    // ↓ from the empty prompt goes into the list on main, ↓↓ to tests, Enter shows it.
    for code in [KeyCode::Down, KeyCode::Down, KeyCode::Down, KeyCode::Enter] {
        press(&mut app, code, now);
    }
    insta::assert_snapshot!(screen(&mut app, now), @"
    ◆ agent tests  ·  started by go

    ❯ run them

    * Running cargo test

    * Working (Ns · ctrl-c to interrupt)

    ────────────────────────────────────────────────────────────
    ❯ Viewing agent tests · a task you type goes to go
    ────────────────────────────────────────────────────────────
      default mode · shift+tab to cycle

      * go  Messaging agent tests · run them
        ├ ✓ scout  done
        └ * tests  current · Running cargo test
      ↓ manage · esc back to go
    ");
}

#[test]
fn an_agent_shows_how_long_it_worked_and_its_tokens_after_what_it_does() {
    let now = Instant::now();
    let mut app = App::new(60, 20, vec![]);
    started(&mut app, "s1", "scout");
    let agent = |event| in_session("s1", event);
    app.frame(agent(Event::TaskStarted("look".into())), now);
    app.frame(agent(Event::Spent(54_120)), now);
    let spinner = |_| Span::raw("*");
    let later = now + std::time::Duration::from_secs(68);
    let row = style::plain(&render(&app, later, &spinner)[1]);
    // The seconds in two digits, as every duration of the interface reads, at the right edge.
    assert_eq!(
        row,
        "    └ * scout  thinking                1m 08s · 54.1k tokens"
    );
}

#[test]
fn wide_tree_keeps_names_readable_and_statistics_at_the_right_edge() {
    let now = Instant::now();
    let mut app = App::new(160, 30, vec![]);
    app.event(Event::TaskStarted("polish session navigation".into()), now);
    started(&mut app, "s1", "inline-picker");
    app.frame(in_session("s1", Event::TaskStarted("choose".into())), now);
    app.frame(in_session("s1", Event::Spent(17_500)), now);
    let spinner = |_| Span::raw("*");
    let shown: Vec<String> = render(&app, now + std::time::Duration::from_secs(13), &spinner)
        .iter()
        .map(style::plain)
        .collect();
    assert!(shown[0].contains("polish session navigation"), "{shown:?}");
    assert!(shown[1].contains("inline-picker"), "{shown:?}");
    // Each row's time ends where the footer's spend does, so the rows read as one table.
    for row in &shown[..2] {
        assert_eq!(text_width(row), 160, "{row:?}");
        assert!(
            row.trim_end().ends_with("13s") || row.contains("tokens"),
            "{row:?}"
        );
    }
}

#[test]
fn many_agents_show_a_window_around_the_one_shown() {
    let mut app = App::new(60, 20, vec![]);
    for n in 0..8 {
        started(&mut app, &format!("s{n}"), &format!("a{n}"));
    }
    app.show("s7");
    let shown = rows(&app);
    assert!(shown[1].contains("↑ 3 more"), "{shown:?}");
    assert!(shown[6].starts_with("    └ ○ a7"), "{shown:?}");
    assert_eq!(shown.len(), 8, "{shown:?}");
}

fn lanes(app: &mut App, lanes: &[(&str, &str)]) {
    for (id, name) in lanes {
        let (id, name) = (id.to_string(), name.to_string());
        app.event(Event::LaneStarted { id, name }, Instant::now());
    }
}

#[test]
fn tree_shows_lane_rows_with_time_and_tokens() {
    let now = Instant::now();
    let mut app = App::new(72, 20, vec![]);
    lanes(&mut app, &[("A", "fix the flaky"), ("B", "refactor")]);
    app.frame(in_session("A", Event::TaskStarted("fix it".into())), now);
    app.frame(in_session("A", Event::Spent(54_100)), now);
    let scout = Event::Started {
        id: "s1".into(),
        name: "scout".into(),
    };
    app.frame(in_session("A", scout), now);
    app.frame(in_session("s1", Event::TaskStarted("look".into())), now);
    app.frame(in_session("s1", Event::Spent(12_000)), now);
    app.frame(
        in_session("B", Event::TaskStarted("refactor it".into())),
        now,
    );
    let ended = Event::TaskEnded {
        status: "done".into(),
        millis: 242_000,
        turns: 3,
        cost: 0.0,
    };
    app.frame(in_session("B", ended), now);
    app.frame(in_session("B", Event::Spent(81_000)), now);
    let spinner = |_| Span::raw("*");
    let later = now + std::time::Duration::from_secs(72);
    let rows: Vec<String> = render(&app, later, &spinner)
        .iter()
        .map(style::plain)
        .collect();
    insta::assert_snapshot!(rows.join("\n"), @"
    * fix the flaky  working                         1m 12s · 54.1k tokens
      └ * scout  thinking                            1m 12s · 12.0k tokens
    ↓ manage · ← 1 other session (1 finished)
    ");
}

#[test]
fn waiting_for_you_detail_when_held() {
    let now = Instant::now();
    let mut app = App::new(60, 20, vec![]);
    lanes(&mut app, &[("A", "first"), ("B", "add tabs")]);
    let choose = protocol::Asked::Choose {
        title: "Login › Provider".into(),
        options: vec![("Yes".into(), String::new())],
        marked: None,
    };
    app.asked(1, "B", choose, now);
    // Another session is not listed under the prompt: one line says what waits there.
    assert_eq!(rows(&app), ["  ← 1 other session (1 waiting for you)"]);
}

#[test]
fn other_sessions_without_agents_take_one_line() {
    let now = Instant::now();
    let mut app = App::new(60, 20, vec![]);
    lanes(&mut app, &[("A", "first"), ("B", "second"), ("C", "third")]);
    app.frame(in_session("B", Event::TaskStarted("go".into())), now);
    assert_eq!(rows(&app), ["  ← 2 other sessions (1 working)"]);
    // ↓ has nothing to choose there: it stays the composer's.
    press(&mut app, KeyCode::Down, now);
    assert_eq!(app.views.focus, None);
}

#[test]
fn another_sessions_agents_are_not_listed() {
    let now = Instant::now();
    let mut app = App::new(60, 20, vec![]);
    lanes(&mut app, &[("A", "first"), ("B", "second")]);
    app.frame(
        in_session(
            "B",
            Event::Started {
                id: "s1".into(),
                name: "scout".into(),
            },
        ),
        now,
    );
    assert_eq!(rows(&app), ["  ← 1 other session"]);
    app.show("B");
    assert_eq!(
        rows(&app),
        [
            "  ● second  idle",
            "    └ ○ scout  starting",
            "  ↓ manage · ← 1 other session"
        ]
    );
}

#[test]
fn ctrl_c_in_the_tree_gives_the_keys_back_and_stops_nothing() {
    let now = Instant::now();
    let mut app = App::new(60, 20, vec![]);
    app.event(Event::TaskStarted("go".into()), now);
    started(&mut app, "s1", "scout");
    press(&mut app, KeyCode::Down, now);
    assert_eq!(app.views.row(), Some(0));
    assert_eq!(app.input(crate::test_support::ctrl('c'), now), None);
    assert_eq!(app.views.focus, None);
    assert!(app.busy().is_some(), "the task goes on");
}
