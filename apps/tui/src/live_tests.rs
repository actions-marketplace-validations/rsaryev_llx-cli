use std::time::{Duration, Instant};

use crossterm::event::KeyCode;

use super::*;
use crate::style;
use crate::test_support::{
    approve, call, label, live_area as shown, press, queue, sent, sent_texts, typed,
};
use protocol::Event;

fn app() -> App {
    let mut app = App::new(
        60,
        20,
        vec![
            ("mode".into(), "show or switch the mode".into()),
            ("model".into(), "pick the model".into()),
        ],
    );
    app.modes = crate::test_support::modes();
    app
}

#[test]
fn idle_shows_the_current_mode_and_cycle_hint() {
    let app = app();
    let (lines, cursor) = render(&app, Instant::now());
    assert_eq!(
        lines.iter().map(style::plain).collect::<Vec<_>>(),
        [
            "─".repeat(60).as_str(),
            "❯ Type a task, / for commands",
            "─".repeat(60).as_str(),
            "  default mode · shift+tab to cycle"
        ]
    );
    // The rules are dim: they frame the composer, they are not read.
    assert!(lines[0].spans.iter().all(|span| span.style == style::dim()));
    assert_eq!(cursor, Some(Cursor { row: 1, column: 2 }));
}

#[test]
fn what_the_session_spent_sits_at_the_right_of_the_footer_once_it_spent() {
    let mut app = App::new(80, 20, vec![]);
    let now = Instant::now();
    let footer = |app: &App| {
        shown(app, now)
            .lines()
            .last()
            .unwrap_or_default()
            .to_string()
    };
    assert_eq!(
        footer(&app),
        "  default mode · shift+tab to cycle",
        "nothing spent yet"
    );
    let total = protocol::Total {
        input_tokens: 24_100,
        cached_tokens: 21_000,
        output_tokens: 1_200,
        cost: 0.042,
    };
    app.event(Event::Total(total), now);
    assert_eq!(
        footer(&app),
        "  default mode · shift+tab to cycle    24.1k in · 87% cached · 1.2k out · $0.042"
    );
    assert_eq!(
        footer(&app).chars().count(),
        80,
        "it ends at the terminal's edge"
    );
    // A narrow terminal keeps the footer's own words.
    app.width = 50;
    assert_eq!(footer(&app), "  default mode · shift+tab to cycle");
}

#[test]
fn mode_stays_visible_while_typing_and_updates_for_every_mode() {
    let now = Instant::now();
    let mut app = app();
    app.event(Event::TaskStarted("work".into()), now);
    typed(&mut app, "follow up", now);
    for mode in ["default", "plan", "bypass", "autonomous"] {
        app.event(Event::Mode(mode.into()), now);
        let screen = shown(&app, now);
        assert!(
            screen.contains(&format!("{mode} mode · shift+tab to cycle")),
            "{screen}"
        );
        assert!(screen.contains("❯ follow up"), "{screen}");
    }
}

#[test]
fn working_shows_the_call_its_steps_the_time_and_the_queue() {
    let start = Instant::now();
    let mut app = app();
    app.event(Event::TaskStarted("fix the parser".into()), start);
    let wait = label("Waiting for", "Waited for", "agent scout");
    app.event(call("c1", "agents", &wait), start);
    // What the agent it waits for did, as the server says it under the call (`agents/watch.rs`).
    app.event(Event::Step("scout: Ran cargo test".into()), start);
    // Tab while it works keeps the entry for after the task; the session says the mode.
    typed(&mut app, "then update the changelog", start);
    press(&mut app, KeyCode::Tab, start);
    let later = sent(&mut app);
    assert!(later[0].later, "for after the task");
    // The server keeps it, and says so.
    app.event(queue(false, &[], &[&later[0].text], &[]), start);
    app.event(Event::Mode("plan".into()), start);
    insta::assert_snapshot!(shown(&app, start + Duration::from_millis(12_300)), @"
    ⠸ Waiting for agent scout
      └ scout: Ran cargo test

    ⠸ Working (12s · esc stop)
      ⏎ then update the changelog

    ────────────────────────────────────────────────────────────
    ❯ Type a task, / for commands
    ────────────────────────────────────────────────────────────
      plan mode · shift+tab to cycle
    ");
}

#[test]
fn slash_lists_the_commands_under_the_prompt() {
    let now = Instant::now();
    let mut app = app();
    typed(&mut app, "/", now);
    insta::assert_snapshot!(shown(&app, now), @"
    ────────────────────────────────────────────────────────────
    ❯ /
    ────────────────────────────────────────────────────────────
      /mode       show or switch the mode
      /model      pick the model
      default mode · shift+tab to cycle
    ");
}

#[test]
fn mode_breadcrumb_and_suggestions_fit_a_narrow_terminal() {
    let now = Instant::now();
    let mut app = app();
    app.width = 20;
    typed(&mut app, "/mode ", now);
    let (lines, _) = render(&app, now);
    assert!(shown(&app, now).contains("mode  ›"));
    assert!(shown(&app, now).contains("review"));
    assert!(lines.iter().all(|line| crate::wrap::line_width(line) <= 20));
}

#[test]
fn a_long_command_in_an_approval_wraps_under_its_indent() {
    // The question takes the composer's place, and the call's and the task's rows wait.
    let now = Instant::now();
    let mut app = App::new(50, 20, vec![]);
    app.event(Event::TaskStarted("x".into()), now);
    let summary = "cd /Users/me/site && wc -l index.html v2.html style.css && grep -n -i 'viewport' index.html";
    app.asked(1, "", approve("bash", summary), now);
    let (lines, _) = render(&app, now);
    assert!(lines.iter().all(|line| crate::wrap::line_width(line) <= 50));
    insta::assert_snapshot!(shown(&app, now), @"
    • Run this command?
      cd /Users/me/site && wc -l index.html v2.html
      style.css && grep -n -i 'viewport' index.html

    ❯ 1. Yes
      2. Yes, and don't ask again for bash
      3. No, and tell llx what to do differently

      enter select · esc no · ctrl-c stop
    ");
}

#[test]
fn the_quit_hint_goes_once_its_window_has_passed() {
    let now = Instant::now();
    let mut app = app();
    app.input(crate::test_support::ctrl('c'), now);
    assert!(shown(&app, now).contains("ctrl-c again to quit"));
    let later = shown(&app, now + Duration::from_secs(3));
    assert!(!later.contains("ctrl-c again to quit"), "{later}");
}

#[test]
fn what_waits_reads_as_the_transcript_shows_it() {
    let now = Instant::now();
    let mut app = app();
    app.event(Event::TaskStarted("x".into()), now);
    let shift = |code| {
        let key = crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::SHIFT);
        crate::event::Input::Key(key)
    };
    app.input(shift(KeyCode::BackTab), now);
    app.input(shift(KeyCode::Up), now);
    // The server keeps them for after the task, behind them what came back from a monitor.
    let sent = sent_texts(&mut app);
    let sent: Vec<&str> = sent.iter().map(String::as_str).collect();
    app.event(queue(false, &[], &sent, &["≋ m · x"]), now);
    let screen = shown(&app, now);
    assert!(!screen.contains('\u{1b}'), "{screen}");
    insta::assert_snapshot!(screen, @"
    ⠋ Working (0s · esc stop)
      ⏎ shift+tab · next mode
      ⏎ more reasoning
      ≋ m · x

    ────────────────────────────────────────────────────────────
    ❯ Type a task, / for commands
    ────────────────────────────────────────────────────────────
      default mode · shift+tab to cycle
    ");
}

#[test]
fn esc_closes_the_commands_slash_offers_and_keeps_other_text() {
    let now = Instant::now();
    let mut app = app();
    typed(&mut app, "/mo", now);
    press(&mut app, KeyCode::Esc, now);
    assert_eq!(
        shown(&app, now),
        format!(
            "{rule}\n❯ Type a task, / for commands\n{rule}\n  default mode · shift+tab to cycle",
            rule = "─".repeat(60)
        )
    );
    // An `@` file list is part of the words: Esc keeps them.
    app.files = crate::mention::Files::with(vec!["src/main.rs".into()]);
    typed(&mut app, "see @sr", now);
    assert!(shown(&app, now).contains("src/main.rs"));
    press(&mut app, KeyCode::Esc, now);
    assert!(shown(&app, now).contains("\n❯ see @sr\n"));
}

#[test]
fn an_agents_name_cannot_reach_the_terminal_from_the_composer() {
    let now = Instant::now();
    let mut app = app();
    let name = "sc\u{1b}]0;x\u{7}out".to_string();
    app.event(
        Event::Started {
            id: "s1".into(),
            name,
        },
        now,
    );
    app.show("s1");
    let screen = shown(&app, now);
    assert!(screen.contains("❯ Viewing agent sc"), "{screen}");
    assert!(!screen.contains('\u{1b}'), "{screen:?}");
}

#[test]
fn the_footer_counts_the_commands_and_monitors_the_session_shown_runs_in_the_background() {
    let mut app = app();
    let now = Instant::now();
    let row = |id: &str, kind| protocol::JobRow {
        id: id.into(),
        name: id.into(),
        kind,
        detail: "x".into(),
        until_at: Some(u64::MAX),
        next: None,
        events: 0,
    };
    let footer = |app: &App| {
        let (lines, _) = render(app, now);
        (lines.iter().map(style::plain))
            .find(|line| line.starts_with("  default mode"))
            .unwrap_or_default()
    };
    assert_eq!(footer(&app), "  default mode · shift+tab to cycle");
    let rows = vec![
        row("a", protocol::JobKind::Shell),
        row("b", protocol::JobKind::Shell),
        row("c", protocol::JobKind::Monitor),
        row("d", protocol::JobKind::Schedule),
    ];
    app.event(Event::Jobs(rows), now);
    assert_eq!(
        footer(&app),
        "  default mode · shift+tab to cycle · 2 shells, 1 monitor"
    );
    app.event(Event::Jobs(vec![row("c", protocol::JobKind::Monitor)]), now);
    assert_eq!(
        footer(&app),
        "  default mode · shift+tab to cycle · 1 monitor"
    );
}
