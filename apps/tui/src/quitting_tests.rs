use std::time::Instant;

use crossterm::event::KeyCode;
use protocol::Running;

use crate::app::{Action, App};
use crate::test_support::{ctrl, in_session as from, press, render, typed};
use protocol::Event;

/// Lanes A (shown) and B; B works when `b_works`.
fn two(b_works: bool, now: Instant) -> App {
    let mut app = App::new(100, 30, vec![]);
    for (id, name) in [("A", "first"), ("B", "fix the flaky jobs test")] {
        let (id, name) = (id.to_string(), name.to_string());
        app.event(Event::LaneStarted { id, name }, now);
    }
    if b_works {
        app.frame(from("B", Event::TaskStarted("fix it".into())), now);
    }
    app
}

fn running(kind: &str, name: &str) -> Running {
    Running {
        kind: kind.into(),
        name: name.into(),
    }
}

/// What the server says runs: B's task, a shell and a monitor.
fn busy(app: &mut App) {
    app.work = Box::new(|| {
        vec![
            running("session", "fix the flaky jobs test"),
            running("shell", "cargo build --release"),
            running("monitor", "watch-ci"),
        ]
    });
}

#[test]
fn the_question_names_what_would_stop_and_offers_to_exit_or_stay() {
    let now = Instant::now();
    let mut app = two(true, now);
    busy(&mut app);
    assert_eq!(app.input(ctrl('c'), now), None);
    assert_eq!(app.input(ctrl('c'), now), None, "asked first");
    let shown = render(&mut app, now);
    for said in [
        "Background work is running",
        "The following will stop when you exit:",
        "session · fix the flaky jobs test",
        "shell · cargo build --release",
        "monitor · watch-ci",
        "1. Exit and stop tasks",
        "2. Stay",
    ] {
        assert!(shown.contains(said), "{said}: {shown}");
    }
    assert!(
        !shown.contains("background and exit"),
        "in process: {shown}"
    );
    assert_eq!(
        app.input(crate::test_support::key(KeyCode::Char('2')), now),
        None
    );
    assert!(app.questions.is_empty(), "2 stays");
    app.input(ctrl('c'), now);
    app.input(ctrl('c'), now);
    assert_eq!(
        app.input(crate::test_support::key(KeyCode::Char('1')), now),
        Some(Action::QuitNow)
    );
}

#[test]
fn a_name_with_a_control_sequence_is_shown_escaped() {
    let now = Instant::now();
    let mut app = two(true, now);
    app.work = Box::new(|| vec![running("monitor", "watch\u{1b}-ci")]);
    app.input(ctrl('c'), now);
    app.input(ctrl('c'), now);
    let shown = render(&mut app, now);
    assert!(shown.contains("monitor · watch\\u{1b}-ci"), "{shown}");
}

#[test]
fn the_quit_question_offers_to_exit_and_stop_or_to_stay() {
    let now = Instant::now();
    let mut app = two(false, now);
    busy(&mut app);
    let digit = |c| crate::test_support::key(KeyCode::Char(c));
    assert_eq!(app.input(ctrl('d'), now), None);
    let shown = render(&mut app, now);
    for said in ["1. Exit and stop tasks", "2. Stay"] {
        assert!(shown.contains(said), "{said}: {shown}");
    }
    assert!(!shown.contains("background and exit"), "{shown}");
    assert_eq!(app.input(digit('2'), now), None);
    assert!(app.questions.is_empty(), "2 stays");
    app.input(ctrl('d'), now);
    assert_eq!(press(&mut app, KeyCode::Esc, now), None);
    assert!(app.questions.is_empty(), "Esc stays");
    app.input(ctrl('d'), now);
    assert_eq!(press(&mut app, KeyCode::Enter, now), Some(Action::QuitNow));
}

#[test]
fn ctrl_d_with_nothing_running_quits() {
    let now = Instant::now();
    let mut app = two(false, now);
    assert_eq!(app.input(ctrl('d'), now), Some(Action::Quit));
}

#[test]
fn exit_with_work_running_asks() {
    let now = Instant::now();
    let mut app = two(true, now);
    busy(&mut app);
    typed(&mut app, "/exit", now);
    assert_eq!(press(&mut app, KeyCode::Enter, now), None);
    let shown = render(&mut app, now);
    assert!(shown.contains("Background work is running"), "{shown}");
    assert_eq!(press(&mut app, KeyCode::Esc, now), None, "Esc stays");
    assert!(app.questions.is_empty());
}

#[test]
fn ctrl_c_while_stopping_quits_only_when_nothing_else_works() {
    let now = Instant::now();
    let mut alone = two(false, now);
    alone.frame(from("A", Event::TaskStarted("a's task".into())), now);
    assert_eq!(alone.input(ctrl('c'), now), Some(Action::Stop("A".into())));
    assert_eq!(
        alone.input(ctrl('c'), now),
        Some(Action::Stop("A".into())),
        "stopped again, it quits, as before"
    );
    let mut app = two(true, now);
    busy(&mut app);
    app.frame(from("A", Event::TaskStarted("a's task".into())), now);
    assert_eq!(app.input(ctrl('c'), now), Some(Action::Stop("A".into())));
    assert_eq!(app.input(ctrl('c'), now), None, "B works: asked first");
    assert!(render(&mut app, now).contains("Background work is running"));
}
