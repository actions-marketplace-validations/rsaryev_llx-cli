use std::time::Instant;

use crossterm::event::KeyCode;
use protocol::{Command, Event, JobEnd, JobKind, JobRow};

use crate::app::App;
use crate::test_support::{Term, commands, in_session, press};

/// A background command's row: running, or ended with `ended`.
fn shell(ended: Option<JobEnd>) -> JobRow {
    JobRow {
        id: "job-1".into(),
        name: "nap".into(),
        kind: JobKind::Shell,
        detail: "while :; do echo tick; sleep 1; done".into(),
        until_at: Some(u64::MAX / 2),
        next: None,
        events: 0,
        ended,
    }
}

/// A lane `watch-lane` shown, running `row` in the background.
fn with(row: JobRow, now: Instant) -> App {
    let mut app = App::new(70, 24, vec![]);
    let (id, name) = ("watch-lane".to_string(), String::new());
    app.event(Event::LaneStarted { id, name }, now);
    app.frame(in_session("watch-lane", Event::Jobs(vec![row])), now);
    app
}

fn stop_job() -> Command {
    Command::StopJob {
        lane: "watch-lane".into(),
        job: "job-1".into(),
    }
}

#[test]
fn enter_on_a_commands_row_shows_what_it_prints_as_it_prints() {
    let now = Instant::now();
    let mut app = with(shell(None), now);
    let mut term = Term::new(&app);
    // ↓ to its row, Enter opens it: what it printed is asked for, once a second.
    press(&mut app, KeyCode::Down, now);
    press(&mut app, KeyCode::Enter, now);
    app.poll(now);
    let asked = Command::JobOutput {
        lane: "watch-lane".into(),
        job: "job-1".into(),
    };
    assert_eq!(commands(&mut app), std::slice::from_ref(&asked));
    app.poll(now);
    assert_eq!(commands(&mut app), [], "not twice in a second");
    let output = Event::JobOutput {
        job: "job-1".into(),
        text: "tick\ntick".into(),
    };
    app.frame(in_session("watch-lane", output), now);
    let screen = term.draw(&mut app, now);
    assert!(screen.contains("$ nap · running · "), "{screen}");
    assert!(screen.contains("while :; do echo tick"), "{screen}");
    assert!(screen.contains("  tick\n  tick"), "{screen}");
    assert!(screen.ends_with("x stop · esc back"), "{screen}");
    assert!(!screen.contains("Type a task"), "{screen}");
    app.poll(now + std::time::Duration::from_secs(1));
    assert_eq!(commands(&mut app), [asked]);
    // `x` stops it; Esc goes back to the prompt.
    press(&mut app, KeyCode::Char('x'), now);
    assert_eq!(commands(&mut app), [stop_job()]);
    press(&mut app, KeyCode::Esc, now);
    let screen = term.draw(&mut app, now);
    assert!(screen.contains("Type a task"), "{screen}");
}

#[test]
fn a_command_stopped_stays_listed_in_red_with_how_and_is_removed_with_x() {
    let now = Instant::now();
    let stopped = JobEnd {
        how: "Stopped: by the person".into(),
        failed: true,
    };
    let mut app = with(shell(Some(stopped)), now);
    let mut term = Term::new(&app);
    let screen = term.draw(&mut app, now);
    assert!(
        screen.contains("✗ nap") && screen.contains("Stopped: by the person"),
        "{screen}"
    );
    assert!(!screen.contains("1 shell"), "nothing of it runs:\n{screen}");
    press(&mut app, KeyCode::Down, now);
    let screen = term.draw(&mut app, now);
    assert!(
        screen.ends_with("enter open · x remove · esc back"),
        "{screen}"
    );
    // Opened, it says how it ended; `x` removes it, and as its row goes, so does the view.
    press(&mut app, KeyCode::Enter, now);
    let screen = term.draw(&mut app, now);
    assert!(
        screen.contains("✗ nap · Stopped: by the person"),
        "{screen}"
    );
    assert!(screen.ends_with("x remove · esc back"), "{screen}");
    press(&mut app, KeyCode::Char('x'), now);
    assert_eq!(commands(&mut app), [stop_job()]);
    app.frame(in_session("watch-lane", Event::Jobs(vec![])), now);
    let screen = term.draw(&mut app, now);
    assert!(screen.contains("Type a task"), "{screen}");
    assert!(!screen.contains("nap"), "{screen}");
}

#[test]
fn a_command_that_ended_well_reads_as_done_not_failed() {
    let now = Instant::now();
    let done = JobEnd {
        how: "Exit code: 0".into(),
        failed: false,
    };
    let mut app = with(shell(Some(done)), now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert!(
        screen.contains("✓ nap") && screen.contains("Exit code: 0"),
        "{screen}"
    );
}

#[test]
fn how_a_command_runs_or_ended_ends_at_the_right_edge_as_the_footers_spend_does() {
    let now = Instant::now();
    let stopped = JobEnd {
        how: "Stopped: by the person".into(),
        failed: true,
    };
    let row = JobRow {
        detail: "sleep 30".into(),
        ..shell(Some(stopped))
    };
    let mut app = with(row, now);
    let screen = Term::new(&app).draw(&mut app, now);
    let row = screen.lines().find(|line| line.contains("✗ nap")).unwrap();
    assert!(row.ends_with(" Stopped: by the person"), "{screen}");
    assert_eq!(crate::wrap::width(row), 70, "{screen}");
    // The command stays beside its name, then the room up to it.
    assert!(row.contains("nap  sleep 30   "), "{screen}");
}

/// A monitor's row: waiting, its checks so far, or ended with `ended`.
fn monitor(checks: usize, ended: Option<JobEnd>) -> JobRow {
    JobRow {
        name: "ready".into(),
        kind: JobKind::Monitor,
        detail: "curl -sf localhost:3000/health".into(),
        events: checks,
        ..shell(ended)
    }
}

#[test]
fn a_monitor_waiting_shows_its_checks_and_opens_to_what_its_last_check_printed() {
    let now = Instant::now();
    let mut app = with(monitor(3, None), now);
    let mut term = Term::new(&app);
    let screen = term.draw(&mut app, now);
    let row = (screen.lines())
        .find(|line| line.contains("≋ ready"))
        .unwrap_or_default();
    assert!(row.contains("curl -sf localhost:300"), "{screen}");
    assert!(row.contains("3 checks · "), "{screen}");
    assert!(screen.contains("· 1 monitor"), "{screen}");
    press(&mut app, KeyCode::Down, now);
    let screen = term.draw(&mut app, now);
    assert!(
        screen.contains("enter open · x stop · esc back"),
        "{screen}"
    );
    press(&mut app, KeyCode::Enter, now);
    app.poll(now);
    let asked = Command::JobOutput {
        lane: "watch-lane".into(),
        job: "job-1".into(),
    };
    assert_eq!(commands(&mut app), [asked]);
    let output = Event::JobOutput {
        job: "job-1".into(),
        text: "curl: (7) Failed to connect".into(),
    };
    app.frame(in_session("watch-lane", output), now);
    let screen = term.draw(&mut app, now);
    assert!(screen.contains("≋ ready · 3 checks · "), "{screen}");
    assert!(screen.contains("  curl: (7) Failed to connect"), "{screen}");
    assert!(screen.ends_with("x stop · esc back"), "{screen}");
}

#[test]
fn a_monitor_that_ended_stays_listed_with_how_red_unless_it_was_met() {
    let now = Instant::now();
    let met = JobEnd {
        how: "met after 4 checks (32 s)".into(),
        failed: false,
    };
    let mut app = with(monitor(4, Some(met)), now);
    let mut term = Term::new(&app);
    let screen = term.draw(&mut app, now);
    assert!(screen.contains("✓ ready"), "{screen}");
    assert!(screen.contains("met after 4 checks (32 s)"), "{screen}");
    assert!(
        !screen.contains("1 monitor"),
        "nothing of it runs: {screen}"
    );
    let timed_out = JobEnd {
        how: "timed out after 30 checks (5 min)".into(),
        failed: true,
    };
    let mut app = with(monitor(30, Some(timed_out)), now);
    let screen = term.draw(&mut app, now);
    assert!(screen.contains("✗ ready"), "{screen}");
    press(&mut app, KeyCode::Down, now);
    let screen = term.draw(&mut app, now);
    assert!(
        screen.contains("enter open · x remove · esc back"),
        "{screen}"
    );
}
