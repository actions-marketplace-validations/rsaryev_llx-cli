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
