use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::event::Input;
use crate::test_support::{
    Term, call, commands, done, in_session, label, press, result, sent, typed,
};
use protocol::Command;

fn app() -> App {
    App::new(60, 20, vec![])
}

fn in_agent(app: &mut App, id: &str, event: Event, now: Instant) {
    app.frame(in_session(id, event), now);
}

/// A main session that started `scout` (session `id`), which ran one call.
fn with_scout_as(id: &str, now: Instant) -> App {
    let mut app = app();
    app.event(Event::Note("main says hi".into()), now);
    app.event(
        Event::Started {
            id: id.into(),
            name: "scout".into(),
        },
        now,
    );
    in_agent(&mut app, id, Event::TaskStarted("find it".into()), now);
    in_agent(
        &mut app,
        id,
        result(
            "c1",
            "read",
            &label("Reading", "Read", "src/lib.rs"),
            done(""),
        ),
        now,
    );
    app
}

fn with_scout(now: Instant) -> App {
    with_scout_as("s1", now)
}

fn keys(app: &mut App, codes: &[KeyCode], now: Instant) {
    for &code in codes {
        press(app, code, now);
    }
}

/// A row of the tree chosen with `❯` (`❯ ○ scout …`): its session's name.
fn tree_row(line: &str) -> Option<&str> {
    let mut words = line.strip_prefix("❯ ")?.split_whitespace();
    let mut glyph = words.next()?;
    if ["├", "└"].contains(&glyph) {
        glyph = words.next()?;
    }
    (glyph.chars().count() == 1).then_some(())?;
    words.next()
}

/// The session chosen in the tree, while the list has the keys.
fn chosen(screen: &str) -> Option<&str> {
    screen.lines().find_map(tree_row)
}

/// The composer's row: the last `❯` row that is not one of the tree's.
fn composer(screen: &str) -> &str {
    screen
        .lines()
        .rfind(|line| line.starts_with("❯ ") && tree_row(line).is_none())
        .unwrap_or("")
}

/// The tree's last row: how to use it now.
fn hint(screen: &str) -> &str {
    screen.lines().last().unwrap_or("").trim()
}

#[test]
fn an_agents_transcript_waits_in_its_view_until_it_is_shown() {
    let now = Instant::now();
    let mut app = with_scout(now);
    let mut term = Term::new(&app);
    let main = term.draw(&mut app, now);
    assert!(
        main.contains("main says hi") && !main.contains("src/lib.rs"),
        "{main}"
    );
    // ↓ from the empty prompt goes into the list, on main; ↓ again to scout, Enter shows it.
    press(&mut app, KeyCode::Down, now);
    let list = term.draw(&mut app, now);
    assert_eq!(chosen(&list), Some("New"), "{list}");
    keys(&mut app, &[KeyCode::Down, KeyCode::Enter], now);
    let scout = term.draw(&mut app, now);
    assert!(scout.starts_with("◆ agent scout  ·  started by"), "{scout}");
    assert!(
        scout.contains("find it") && scout.contains("Read src/lib.rs"),
        "{scout}"
    );
    assert!(
        !scout.contains("main says hi"),
        "the screen was cleared:\n{scout}"
    );
    assert!(
        composer(&scout).starts_with("❯ Viewing agent scout"),
        "{scout}"
    );
    // While it is shown, what it does next is printed at once; the main session's is kept.
    in_agent(&mut app, "s1", Event::Answer("Found.".into()), now);
    app.event(Event::Note("main again".into()), now);
    let more = term.draw(&mut app, now);
    assert!(
        more.contains("Found.") && !more.contains("main again"),
        "{more}"
    );
    // ← goes back to main, and gives the keys back to the composer.
    press(&mut app, KeyCode::Left, now);
    let back = term.draw(&mut app, now);
    assert!(
        back.contains("main says hi") && back.contains("main again"),
        "{back}"
    );
    assert!(!back.contains("Found."), "{back}");
    assert_eq!(
        (chosen(&back), hint(&back)),
        (None, "↓ manage · ← sessions"),
        "{back}"
    );
    assert!(composer(&back).starts_with("❯ Type a task"), "{back}");
}

#[test]
fn the_list_takes_the_keys_only_from_an_empty_prompt_and_gives_them_back() {
    let now = Instant::now();
    let mut app = with_scout(now);
    let mut term = Term::new(&app);
    keys(&mut app, &[KeyCode::Char('x'), KeyCode::Down], now);
    let screen = term.draw(&mut app, now);
    assert_eq!(
        chosen(&screen),
        None,
        "with text, ↓ is the composer's:\n{screen}"
    );
    assert_eq!(composer(&screen), "❯ x");
    press(&mut app, KeyCode::Backspace, now);
    keys(&mut app, &[KeyCode::Down; 3], now);
    let screen = term.draw(&mut app, now);
    assert_eq!(
        chosen(&screen),
        Some("scout"),
        "the last row stays chosen:\n{screen}"
    );
    assert_eq!(hint(&screen), "enter open · x stop · esc back");
    press(&mut app, KeyCode::Right, now);
    let screen = term.draw(&mut app, now);
    assert!(
        screen.starts_with("◆ agent scout"),
        "→ shows it too:\n{screen}"
    );
    keys(&mut app, &[KeyCode::Up, KeyCode::Up], now);
    let screen = term.draw(&mut app, now);
    assert_eq!(
        chosen(&screen),
        None,
        "↑ past the top gives the keys back:\n{screen}"
    );
    assert_eq!(hint(&screen), "↓ manage · esc back to New session");
    press(&mut app, KeyCode::Down, now);
    let screen = term.draw(&mut app, now);
    assert_eq!(
        chosen(&screen),
        Some("scout"),
        "back in the list on the session shown"
    );
    press(&mut app, KeyCode::Char('h'), now);
    let screen = term.draw(&mut app, now);
    assert_eq!(chosen(&screen), None, "{screen}");
    assert_eq!(composer(&screen), "❯ h", "typing goes to the composer");
}

#[test]
fn x_stops_the_agent_chosen() {
    let now = Instant::now();
    let mut app = with_scout_as("scout-1", now);
    keys(
        &mut app,
        &[KeyCode::Down, KeyCode::Down, KeyCode::Char('x')],
        now,
    );
    let halt = Command::Halt {
        id: "scout-1".into(),
    };
    assert_eq!(commands(&mut app), [halt]);
}

#[test]
fn esc_and_a_sent_entry_go_back_to_main() {
    let now = Instant::now();
    let mut app = with_scout(now);
    let mut term = Term::new(&app);
    keys(
        &mut app,
        &[KeyCode::Down, KeyCode::Down, KeyCode::Enter],
        now,
    );
    // Enter gave the keys back to the composer: Esc goes back to main.
    let screen = term.draw(&mut app, now);
    assert!(screen.starts_with("◆ agent scout"), "{screen}");
    press(&mut app, KeyCode::Esc, now);
    let screen = term.draw(&mut app, now);
    assert!(screen.contains("main says hi"), "{screen}");
    assert!(!screen.contains("◆ agent scout"), "{screen}");
    keys(
        &mut app,
        &[KeyCode::Down, KeyCode::Down, KeyCode::Enter],
        now,
    );
    typed(&mut app, "y", now);
    press(&mut app, KeyCode::Enter, now);
    let screen = term.draw(&mut app, now);
    assert!(!screen.contains("◆ agent scout"), "{screen}");
    assert!(screen.contains("main says hi"), "{screen}");
    let sent: Vec<(String, String)> = (sent(&mut app).into_iter())
        .map(|input| (input.session, input.text))
        .collect();
    assert_eq!(
        sent,
        [(String::new(), "y".into())],
        "the entry is the main session's"
    );
}

#[test]
fn an_agent_ends_is_closed_and_is_cleared_with_its_conversation() {
    let now = Instant::now();
    let mut app = with_scout(now);
    let mut term = Term::new(&app);
    in_agent(
        &mut app,
        "s1",
        Event::TaskEnded {
            status: "done".into(),
            millis: 10,
            turns: 2,
            cost: 0.0,
        },
        now,
    );
    let screen = term.draw(&mut app, now);
    assert!(screen.contains("● Agent \"scout\" finished"), "{screen}");
    assert!(screen.contains("└ ✓ scout  done"), "{screen}");
    assert!(!app.moving());
    app.frame(in_session("s1", Event::Closed), now);
    let screen = term.draw(&mut app, now);
    assert!(screen.contains("└ ○ scout  closed"), "{screen}");
    keys(
        &mut app,
        &[KeyCode::Down, KeyCode::Down, KeyCode::Enter],
        now,
    );
    assert!(term.draw(&mut app, now).starts_with("◆ agent scout"));
    app.event(Event::AgentsCleared, now);
    let screen = term.draw(&mut app, now);
    assert!(
        !screen.contains("○ scout") && !screen.contains("manage"),
        "{screen}"
    );
    assert!(screen.contains("main says hi"), "back on main:\n{screen}");
    assert!(composer(&screen).starts_with("❯ Type a task"), "{screen}");
}

#[test]
fn esc_in_an_agents_view_goes_back_before_it_stops_anything() {
    let now = Instant::now();
    let mut app = with_scout(now);
    app.event(Event::TaskStarted("main task".into()), now);
    app.show("s1");
    let esc = || Input::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.input(esc(), now), None);
    assert!(app.shown().parent.is_none());
    assert_eq!(app.input(esc(), now), Some(Action::Stop(String::new())));
}

fn job(id: &str, name: &str) -> protocol::JobRow {
    protocol::JobRow {
        id: id.into(),
        name: name.into(),
        detail: format!("say {name}"),
        until_at: None,
        next: Some("00:00".into()),
        events: 0,
    }
}

#[test]
fn the_list_chooses_only_job_rows_it_draws_and_gives_the_keys_back_when_its_job_ends() {
    let now = Instant::now();
    let mut app = app();
    let ids: Vec<String> = (1..=6)
        .map(|n| format!("tui-list-chooses-drawn-rows-{n}"))
        .collect();
    let rows = ids
        .iter()
        .enumerate()
        .map(|(n, id)| job(id, &format!("job{}", n + 1)))
        .collect();
    app.event(Event::Jobs(rows), now);
    // Into the list, then past its last drawn row.
    keys(&mut app, &[KeyCode::Down; 10], now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert_eq!(chosen(&screen), Some("job5"), "{screen}");
    press(&mut app, KeyCode::Char('x'), now);
    let stop = Command::StopJob {
        lane: String::new(),
        job: ids[4].clone(),
    };
    assert_eq!(commands(&mut app), [stop], "the row chosen, not one hidden");
    // The chosen job ends: no other is chosen in its place, and ← is the prompt's.
    app.event(Event::Jobs(vec![job(&ids[0], "job1")]), now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert_eq!(chosen(&screen), None, "{screen}");
    press(&mut app, KeyCode::Left, now);
    assert_eq!(commands(&mut app), [Command::List], "← is the prompt's");
}

#[test]
fn the_job_chosen_stays_chosen_as_the_lanes_first_agent_starts_above_it() {
    let now = Instant::now();
    let mut app = app();
    app.event(Event::TaskStarted("main task".into()), now);
    app.event(Event::Jobs(vec![job("tui-follows-job", "watch")]), now);
    press(&mut app, KeyCode::Down, now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert_eq!(chosen(&screen), Some("watch"), "{screen}");
    // The lane's first agent: the lane's row and the agent's come above the jobs.
    let started = Event::Started {
        id: "tui-follows-scout".into(),
        name: "scout".into(),
    };
    app.event(started, now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert_eq!(chosen(&screen), Some("watch"), "{screen}");
    press(&mut app, KeyCode::Char('x'), now);
    let stop = Command::StopJob {
        lane: String::new(),
        job: "tui-follows-job".into(),
    };
    assert_eq!(
        commands(&mut app),
        [stop],
        "the job chosen, not the lane's task"
    );
}

#[test]
fn the_job_chosen_stays_chosen_as_a_job_above_it_ends() {
    let now = Instant::now();
    let mut app = app();
    let rows = |names: &[&str]| -> Vec<protocol::JobRow> {
        let id = |name: &str| format!("tui-follows-{name}");
        names.iter().map(|&name| job(&id(name), name)).collect()
    };
    app.event(Event::Jobs(rows(&["jone", "jtwo", "jthree"])), now);
    keys(&mut app, &[KeyCode::Down, KeyCode::Down], now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert_eq!(chosen(&screen), Some("jtwo"), "{screen}");
    app.event(Event::Jobs(rows(&["jtwo", "jthree"])), now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert_eq!(chosen(&screen), Some("jtwo"), "{screen}");
    press(&mut app, KeyCode::Char('x'), now);
    let stop = Command::StopJob {
        lane: String::new(),
        job: "tui-follows-jtwo".into(),
    };
    assert_eq!(commands(&mut app), [stop], "jtwo, not the job after it");
}

#[test]
fn the_job_chosen_stays_chosen_as_another_lane_ends_with_its_jobs() {
    let now = Instant::now();
    let mut app = app();
    for id in ["tui-ends-a", "tui-ends-b", "tui-ends-c"] {
        let (id, name) = (id.to_string(), id.to_string());
        app.event(Event::LaneStarted { id, name }, now);
    }
    let jobs = |lane: &str, name: &str| in_session(lane, Event::Jobs(vec![job(name, name)]));
    app.frame(jobs("tui-ends-b", "bwatch"), now);
    app.frame(jobs("tui-ends-c", "cwatch"), now);
    keys(&mut app, &[KeyCode::Down, KeyCode::Down], now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert_eq!(chosen(&screen), Some("cwatch"), "{screen}");
    // B ends, not shown: its job's row goes from above the one chosen.
    app.frame(in_session("tui-ends-b", Event::LaneEnded), now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert_eq!(chosen(&screen), Some("cwatch"), "{screen}");
    press(&mut app, KeyCode::Char('x'), now);
    let stop = Command::StopJob {
        lane: "tui-ends-c".into(),
        job: "cwatch".into(),
    };
    assert_eq!(commands(&mut app), [stop]);
}

#[test]
fn a_gap_that_takes_the_job_chosen_gives_the_keys_back_to_the_prompt() {
    let now = Instant::now();
    let mut app = app();
    app.event(Event::Jobs(vec![job("tui-gap-job", "watch")]), now);
    press(&mut app, KeyCode::Down, now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert_eq!(chosen(&screen), Some("watch"), "{screen}");
    // What is shown may be incomplete: the jobs start again from what follows.
    app.frame(protocol::ToClient::Gap, now);
    // No list is drawn to choose in: ← is the prompt's, which lists the sessions.
    press(&mut app, KeyCode::Left, now);
    assert_eq!(commands(&mut app), [Command::List]);
}

#[test]
fn the_last_job_ending_gives_the_keys_back_to_the_prompt() {
    let now = Instant::now();
    let mut app = app();
    app.event(Event::Jobs(vec![job("tui-last-job-ends", "watch")]), now);
    press(&mut app, KeyCode::Down, now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert_eq!(chosen(&screen), Some("watch"), "{screen}");
    // Its job ended, and the lane started no agent: no list is drawn to choose in.
    app.event(Event::Jobs(vec![]), now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert!(!screen.contains("watch"), "{screen}");
    press(&mut app, KeyCode::Left, now);
    assert_eq!(commands(&mut app), [Command::List], "← is the prompt's");
}

#[test]
fn another_lane_shown_gives_the_keys_back_to_the_prompt() {
    let now = Instant::now();
    let mut app = app();
    for id in ["tui-shown-a", "tui-shown-b"] {
        let (id, name) = (id.to_string(), id.to_string());
        app.event(Event::LaneStarted { id, name }, now);
    }
    let started = Event::Started {
        id: "tui-shown-scout".into(),
        name: "scout".into(),
    };
    app.frame(in_session("tui-shown-a", started), now);
    keys(&mut app, &[KeyCode::Down, KeyCode::Down], now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert_eq!(chosen(&screen), Some("scout"), "{screen}");
    // The server shows the other lane (one the person opened): it has no agents, no list.
    app.event(Event::Show("tui-shown-b".into()), now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert!(!screen.contains("scout"), "{screen}");
    press(&mut app, KeyCode::Left, now);
    assert_eq!(commands(&mut app), [Command::List], "← is the prompt's");
}

/// Lanes `ids`, the first shown, the last with the job `watch`, chosen in the list.
fn a_job_chosen_in_the_last_of(ids: &[&str], now: Instant) -> App {
    let mut app = app();
    for &id in ids {
        let (id, name) = (id.to_string(), id.to_string());
        app.event(Event::LaneStarted { id, name }, now);
    }
    let last = ids.last().copied().unwrap_or_default();
    app.frame(in_session(last, Event::Jobs(vec![job("j", "watch")])), now);
    press(&mut app, KeyCode::Down, now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert_eq!(chosen(&screen), Some("watch"), "{screen}");
    app
}

#[test]
fn another_lane_shown_gives_the_keys_back_though_the_job_chosen_is_still_drawn() {
    let now = Instant::now();
    let mut app = a_job_chosen_in_the_last_of(&["tui-shown-a", "tui-shown-b"], now);
    // Every lane's jobs are drawn under each: the row stays, but the lane on the screen changed.
    app.event(Event::Show("tui-shown-b".into()), now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert_eq!(chosen(&screen), None, "{screen}");
    press(&mut app, KeyCode::Left, now);
    assert_eq!(commands(&mut app), [Command::List], "← is the prompt's");
}

#[test]
fn the_lane_shown_ending_gives_the_keys_back_as_another_is_shown_in_its_place() {
    let now = Instant::now();
    let ids = ["tui-gone-a", "tui-gone-b", "tui-gone-c"];
    let mut app = a_job_chosen_in_the_last_of(&ids, now);
    // A ends while shown: B is shown in its place, and C's job is still drawn under it.
    app.frame(in_session("tui-gone-a", Event::LaneEnded), now);
    let screen = Term::new(&app).draw(&mut app, now);
    assert!(screen.contains("◷ watch"), "{screen}");
    assert_eq!(chosen(&screen), None, "{screen}");
    press(&mut app, KeyCode::Left, now);
    assert_eq!(commands(&mut app), [Command::List], "← is the prompt's");
}

#[test]
fn an_agent_started_in_a_lane_gets_its_events() {
    let now = Instant::now();
    let mut app = app();
    let lane = |event| in_session("lane", event);
    app.event(
        Event::LaneStarted {
            id: "lane".into(),
            name: String::new(),
        },
        now,
    );
    let started = Event::Started {
        id: "s1".into(),
        name: "scout".into(),
    };
    app.frame(lane(started), now);
    // The agent's own events are in its own name.
    app.frame(in_session("s1", Event::TaskStarted("look".into())), now);
    let mut term = Term::new(&app);
    let screen = term.draw(&mut app, now);
    assert!(screen.contains("scout  thinking"), "{screen}");
    keys(
        &mut app,
        &[KeyCode::Down, KeyCode::Down, KeyCode::Enter],
        now,
    );
    let screen = term.draw(&mut app, now);
    assert!(screen.contains("look"), "{screen}");
}

#[test]
fn tagged_events_reach_their_own_view() {
    let now = Instant::now();
    let mut app = app();
    for id in ["A", "B"] {
        let name = format!("lane {id}");
        app.event(
            Event::LaneStarted {
                id: id.into(),
                name,
            },
            now,
        );
    }
    let lane = |id: &str, event| in_session(id, event);
    let read = call("c1", "read", &label("Reading", "Read", "a.rs"));
    app.frame(lane("A", read), now);
    app.frame(lane("B", Event::Answer("from b".into())), now);
    let (a, b) = (app.views.find("A").unwrap(), app.views.find("B").unwrap());
    let (a, b) = (&app.views.sessions[a], &app.views.sessions[b]);
    assert_eq!((a.running.len(), a.blocks.len()), (1, 0));
    assert_eq!((b.running.len(), b.blocks.len()), (0, 1));
    let text = |lines: Vec<Line>| lines.iter().map(ToString::to_string).collect::<String>();
    let history = |app: &mut App| crate::test_support::rows(app.take_history());
    assert!(!text(history(&mut app)).contains("from b"), "A is shown");
    app.show("B");
    assert!(text(history(&mut app)).contains("from b"));
}

#[test]
fn clearing_a_hidden_lanes_agents_leaves_the_lane_shown() {
    let now = Instant::now();
    let mut app = app();
    for id in ["A", "B"] {
        let name = format!("lane {id}");
        app.event(
            Event::LaneStarted {
                id: id.into(),
                name,
            },
            now,
        );
    }
    let started = Event::Started {
        id: "scout".into(),
        name: "scout".into(),
    };
    app.frame(in_session("A", started), now);
    app.show("B");
    app.take_replay();
    app.frame(in_session("A", Event::AgentsCleared), now);
    assert_eq!(app.views.shown, "B", "the person stays where they are");
    assert!(!app.take_replay());
    assert!(app.views.find("scout").is_none(), "A's agent is gone");
}

#[test]
fn each_sessions_jobs_are_kept_apart_and_stopped_by_their_lane() {
    let now = Instant::now();
    let mut app = App::new(60, 20, vec![]);
    for id in ["jobs-lane-a", "jobs-lane-b"] {
        let (id, name) = (id.to_string(), id.to_string());
        app.event(Event::LaneStarted { id, name }, now);
    }
    let from = |id: &str, rows| in_session(id, Event::Jobs(rows));
    app.frame(from("jobs-lane-a", vec![job("job-1", "a-watch")]), now);
    app.frame(from("jobs-lane-b", vec![job("job-1", "b-watch")]), now);
    let names =
        |app: &App| -> Vec<String> { app.views.jobs.iter().map(|(_, j)| j.name.clone()).collect() };
    assert_eq!(
        names(&app),
        ["a-watch", "b-watch"],
        "the last to report does not win"
    );
    // Into the list: two lanes, then B's job.
    keys(
        &mut app,
        &[KeyCode::Down, KeyCode::Down, KeyCode::Down, KeyCode::Down],
        now,
    );
    press(&mut app, KeyCode::Char('x'), now);
    let stop = Command::StopJob {
        lane: "jobs-lane-b".into(),
        job: "job-1".into(),
    };
    assert_eq!(commands(&mut app), [stop], "B's job-1, not A's");
    app.frame(from("jobs-lane-a", vec![]), now);
    assert_eq!(names(&app), ["b-watch"]);
}

#[test]
fn an_agents_view_names_the_session_that_started_it() {
    let now = Instant::now();
    let mut app = App::new(80, 20, vec![]);
    for (id, name) in [("lane-a", "first"), ("lane-b", "fix the flaky jobs test")] {
        let (id, name) = (id.to_string(), name.to_string());
        app.event(Event::LaneStarted { id, name }, now);
    }
    let started = Event::Started {
        id: "s9".into(),
        name: "scout".into(),
    };
    app.frame(in_session("lane-b", started), now);
    app.show("s9");
    let shown = crate::test_support::history(&mut app);
    assert!(
        shown.contains("started by fix the flaky jobs test"),
        "{shown}"
    );
}

#[test]
fn a_lane_is_named_by_its_first_task() {
    let now = Instant::now();
    let mut app = App::new(80, 20, vec![]);
    let (id, name) = ("lane-n".to_string(), String::new());
    app.event(Event::LaneStarted { id, name }, now);
    let task = |goal: &str| in_session("lane-n", Event::TaskStarted(goal.into()));
    app.frame(task("fix the flaky jobs test\nand more"), now);
    app.frame(task("something else"), now);
    assert_eq!(app.shown().name, "fix the flaky jobs test");
}

#[test]
fn hidden_lane_done_sets_unseen_and_notes_it() {
    let now = Instant::now();
    let mut app = App::new(80, 20, vec![]);
    for (id, name) in [("A", "first"), ("B", "fix the flaky jobs test")] {
        let (id, name) = (id.to_string(), name.to_string());
        app.event(Event::LaneStarted { id, name }, now);
    }
    crate::test_support::history(&mut app);
    let from = |event| in_session("B", event);
    app.frame(from(Event::TaskStarted("fix it".into())), now);
    let ended = Event::TaskEnded {
        status: "done".into(),
        millis: 188_000,
        turns: 3,
        cost: 0.0,
    };
    app.frame(from(ended), now);
    let shown = crate::test_support::history(&mut app);
    assert!(
        shown.contains("● Session \"fix the flaky jobs test\" finished · 3m 08s · ← to open"),
        "{shown}"
    );
    let b = app.views.find("B").unwrap();
    assert!(app.views.sessions[b].unseen);
    app.show("B");
    assert!(!app.views.sessions[b].unseen, "seen once shown");
}

/// A lane's task ends `exhausted` when its turns, its time (`--timeout`) or its budget
/// (`--budget`) ran out, and the status does not say which: the line about it names no one limit.
#[test]
fn a_hidden_lane_a_limit_stopped_says_it_reached_one_not_which() {
    let now = Instant::now();
    let mut app = App::new(80, 20, vec![]);
    for (id, name) in [("A", "first"), ("B", "fix the flaky jobs test")] {
        let (id, name) = (id.to_string(), name.to_string());
        app.event(Event::LaneStarted { id, name }, now);
    }
    crate::test_support::history(&mut app);
    let from = |event| in_session("B", event);
    app.frame(from(Event::TaskStarted("fix it".into())), now);
    let ended = Event::TaskEnded {
        status: "exhausted".into(),
        millis: 188_000,
        turns: 3,
        cost: 0.0,
    };
    app.frame(from(ended), now);
    let shown = crate::test_support::history(&mut app);
    assert!(
        shown.contains("● Session \"fix the flaky jobs test\" reached a limit · 3m 08s"),
        "{shown}"
    );
}

#[test]
fn a_resumed_lane_is_named_by_the_first_line_of_its_first_task() {
    let now = Instant::now();
    let mut app = App::new(80, 20, vec![]);
    let (id, name) = ("first".to_string(), String::new());
    app.event(Event::LaneStarted { id, name }, now);
    let (id, name) = (
        "resumed".to_string(),
        "port the tests\nall of them".to_string(),
    );
    app.event(Event::LaneStarted { id, name }, now);
    let at = app.views.find("resumed").unwrap();
    assert_eq!(app.views.sessions[at].name, "port the tests");
}

#[test]
fn ctrl_c_in_an_agents_view_stops_that_agent_alone_then_goes_back() {
    let now = Instant::now();
    let mut app = with_scout_as("scout-1", now);
    app.event(Event::TaskStarted("main task".into()), now);
    app.show("scout-1");
    let ctrl_c = || Input::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert_eq!(app.input(ctrl_c(), now), None, "the main task goes on");
    let halt = Command::Halt {
        id: "scout-1".into(),
    };
    assert_eq!(commands(&mut app), [halt]);
    assert!(!app.lane().stopping);
    let ended = Event::TaskEnded {
        status: "stopped".into(),
        millis: 10,
        turns: 1,
        cost: 0.0,
    };
    in_agent(&mut app, "scout-1", ended, now);
    // Nothing left to stop in the view: the next goes back to the main one.
    assert_eq!(app.input(ctrl_c(), now), None);
    assert!(app.shown().parent.is_none());
}
