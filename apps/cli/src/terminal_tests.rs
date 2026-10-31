use std::sync::Mutex;
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use llx::test_support::{Link, MAX_LANES, call, gated, said_by_agent, says, served};
use protocol::{Command, Event, Target};
use serde_json::json;
use tui::testing::{Person, State, Window};

use super::test_support::{at_desk, at_desk_with, connected};

// The desk's lanes, each a session of its own, as the person at the interface sees them.

#[test]
fn lane_a_keeps_running_while_lane_b_runs_a_task() {
    let ((to_a, a), (to_b, b)) = (gated(), gated());
    at_desk(&[&a, &b], Some("a's task"), |person| {
        person.until("A asks its model", |_| a.turns() == 1);
        let lane_a = person.first();
        let lane_b = person.new_lane();
        assert_ne!(lane_a, lane_b);
        person.enter("b's task");
        person.until("B asks its model", |_| b.turns() == 1);
        to_b.send(says("B is done.")).unwrap();
        person.until("B ends", |app| app.ended(&lane_b));
        assert_eq!(person.state(&lane_a), Some(State::Working));
        to_a.send(says("A is done.")).unwrap();
        person.until("A ends", |app| app.ended(&lane_a));
    });
}

#[test]
fn each_lane_records_in_its_own_session() {
    let ((to_a, a), (to_b, b)) = (gated(), gated());
    let ids = Mutex::new(vec![]);
    let scratch = at_desk(&[&a, &b], None, |person| {
        person.until("A waits", |app| app.ready());
        let lane_a = person.first();
        to_a.send(says("A is done.")).unwrap();
        person.enter("a's task");
        person.until("A ends", |app| app.ended(&lane_a));
        let lane_b = person.new_lane();
        to_b.send(says("B is done.")).unwrap();
        person.enter("b's task");
        person.until("B ends", |app| app.ended(&lane_b));
        *ids.lock().unwrap() = person.lanes();
    });
    let ids = ids.into_inner().unwrap();
    assert_eq!(scratch.goals(&ids[0]), ["a's task"]);
    assert_eq!(scratch.goals(&ids[1]), ["b's task"]);
}

#[test]
fn new_lane_leaves_the_old_lanes_agents_running() {
    let ((to_a, a), (_to_b, b)) = (gated(), gated());
    at_desk(&[&a, &b], Some("look around"), |person| {
        to_a.send(call("agents", json!({"task": "look", "name": "scout"})))
            .unwrap();
        // The agent and its parent each wait for a turn.
        person.until("the scout works", |app| {
            app.named("scout").is_some() && a.turns() == 3
        });
        person.new_lane();
        assert_eq!(person.seen().named("scout"), Some(State::Working));
        for _ in 0..4 {
            to_a.send(says("Done.")).unwrap();
        }
        let lane_a = person.first();
        person.until("A ends", |app| app.ended(&lane_a));
    });
}

#[test]
fn a_monitor_wakes_only_its_lane() {
    let ((to_a, a), (_to_b, b)) = (gated(), gated());
    at_desk(&[&a, &b], Some("watch"), |person| {
        let monitor = json!({"action": "monitor", "command": "sleep 1; echo up", "name": "m"});
        to_a.send(call("jobs", monitor)).unwrap();
        to_a.send(says("Watching.")).unwrap();
        let lane_a = person.first();
        person.until("A ends", |app| app.ended(&lane_a));
        person.new_lane();
        person.until("the monitor wakes A", |_| a.turns() == 3);
        let woken = a.asked()[2].clone();
        assert!(format!("{woken:?}").contains("up"), "{woken:?}");
        assert_eq!(b.turns(), 0, "B heard nothing");
        to_a.send(says("Seen.")).unwrap();
    });
}

#[test]
fn opening_a_live_session_shows_it() {
    let ((_to_a, a), (_to_b, b)) = (gated(), gated());
    at_desk(&[&a, &b], None, |person| {
        person.until("A waits", |app| app.ready());
        let lane_a = person.first();
        person.new_lane();
        let target = Target::Saved(lane_a.clone());
        person.command(Command::Open { target });
        person.until("A shown", |app| app.shown() == lane_a);
        person.wait(300);
        assert_eq!(person.lanes().len(), 2, "no second lane for A");
    });
}

#[test]
fn ninth_lane_closes_oldest_idle() {
    let chats: Vec<_> = (0..MAX_LANES + 1).map(|_| gated()).collect();
    let models: Vec<_> = chats.iter().map(|(_, chat)| chat).collect();
    at_desk(&models, None, |person| {
        person.until("A waits", |app| app.ready());
        let oldest = person.first();
        for _ in 1..MAX_LANES {
            person.new_lane();
        }
        assert_eq!(person.lanes().len(), MAX_LANES);
        let newest = person.new_lane();
        let lanes = person.lanes();
        assert_eq!(lanes.len(), MAX_LANES, "{lanes:?}");
        assert!(!lanes.contains(&oldest), "the oldest idle one closed");
        assert!(lanes.contains(&newest));
        let shown = person.history();
        assert!(shown.contains("closed the oldest idle session"), "{shown}");
    });
}

#[test]
fn the_desk_never_opens_more_than_max_lanes() {
    let chats: Vec<_> = (0..MAX_LANES + 1).map(|_| gated()).collect();
    let models: Vec<_> = chats.iter().map(|(_, chat)| chat).collect();
    at_desk(&models, None, |person| {
        person.first();
        // Asked past the interface's own limit, as a request that raced it would be.
        for _ in 0..MAX_LANES {
            person.command(Command::Open {
                target: Target::New,
            });
        }
        person.until("the lanes open", |app| app.lanes().len() == MAX_LANES);
        person.wait(1500);
        assert_eq!(person.lanes().len(), MAX_LANES);
    });
}

// How the desk ends: a stop, a panic, a quit, the connection gone.

#[test]
fn stopping_lane_b_leaves_lane_a_running() {
    let ((to_a, a), (to_b, b)) = (gated(), gated());
    at_desk(&[&a, &b], Some("a's task"), |person| {
        person.until("A asks its model", |_| a.turns() == 1);
        let lane_b = person.new_lane();
        person.enter("b's task");
        person.until("B asks its model", |_| b.turns() == 1);
        person.until("B works", |app| app.busy());
        let stopped = person.interrupt();
        assert_eq!(stopped, Some(lane_b.clone()));
        to_b.send(call("read", json!({"path": "."}))).unwrap();
        person.until("B ends", |app| app.ended(&lane_b));
        assert_eq!(
            person.state(&lane_b),
            Some(State::Ended("interrupted".into()))
        );
        let lane_a = person.first();
        assert_eq!(person.state(&lane_a), Some(State::Working));
        to_a.send(says("A is done.")).unwrap();
        person.until("A ends", |app| app.ended(&lane_a));
        assert_eq!(person.state(&lane_a), Some(State::Ended("done".into())));
    });
}

#[test]
fn a_panicking_lane_ends_the_other_lanes() {
    let (_to_a, a) = gated();
    let begun = Instant::now();
    // One model: the second lane's start is a bug, and panics.
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        served(&[&a], None, &|| {}, |Link { frames, to_server }, _| {
            let mut person = Person::new(frames, to_server, MAX_LANES);
            person.until("A waits", |app| app.ready());
            person.enter("/new");
            person.until_over();
            // The interface is gone: what A waits for never comes.
            drop(person);
        })
    }));
    assert!(unwound.is_err(), "the panic reaches the desk's caller");
    assert!(begun.elapsed() < Duration::from_secs(5));
}

#[test]
fn quit_waits_at_most_the_bound() {
    // A model that never answers: only the bound ends the wait.
    let (to_a, a) = gated();
    let sender = Mutex::new(Some(to_a));
    let quit_at = Mutex::new(None);
    let asked_at = Mutex::new(None);
    let quit = || {
        *quit_at.lock().unwrap() = Some(Instant::now());
        // What the process's end does to a request in flight: it is gone.
        sender.lock().unwrap().take();
    };
    at_desk_with(&[&a], Some("hang"), &quit, |person| {
        person.until("A asks its model", |_| a.turns() == 1);
        *asked_at.lock().unwrap() = Some(Instant::now());
        person.quit_now();
        person.until("the quit", |_| quit_at.lock().unwrap().is_some());
    });
    let (asked, quit) = (
        asked_at.into_inner().unwrap(),
        quit_at.into_inner().unwrap(),
    );
    let waited = quit.unwrap() - asked.unwrap();
    assert!(waited >= Duration::from_millis(1900), "{waited:?}");
    assert!(waited < Duration::from_secs(3), "{waited:?}");
}

#[test]
fn quit_with_lanes_that_stop_ends_without_the_hard_exit() {
    let (to_a, a) = gated();
    let quit = || panic!("the lanes ended in time");
    at_desk_with(&[&a], Some("work"), &quit, |person| {
        person.until("A asks its model", |_| a.turns() == 1);
        person.quit_now();
        // A call that outlasts the stop however late it lands (a quick one raced it to a turn).
        let long = call("bash", json!({"command": "sleep 30"}));
        to_a.send(long).unwrap();
        person.until_over();
    });
}

/// The command outlasts the stop, however late it lands: it is killed, and no turn comes after.
#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "nothing tells a lane that it was not asked: time passing does, bounded"
)]
fn a_connection_that_ends_without_background_stops_the_work() {
    let (to_a, a) = gated();
    connected(
        &[&a],
        Some("work"),
        &|| panic!("no hard exit"),
        |mut person, _| {
            person.until("A asks its model", |_| a.turns() == 1);
            // The terminal closed, nothing chosen: as "Exit and stop tasks".
            drop(person);
            std::thread::sleep(Duration::from_millis(300));
            to_a.send(call("read", json!({"path": "."}))).unwrap();
            std::thread::sleep(Duration::from_millis(500));
            let turns = a.turns();
            if turns > 1 {
                // Let the lane end, so the failure is told rather than waited on.
                to_a.send(says("Done.")).unwrap();
            }
            assert_eq!(turns, 1, "the task was stopped, not asked to go on");
        },
    );
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "nothing tells a lane that it was not asked: time passing does, bounded"
)]
fn moved_to_the_background_a_lane_waits_for_the_next_interface_and_keeps_its_jobs() {
    let (to_a, a) = gated();
    connected(
        &[&a],
        Some("remind me"),
        &|| panic!("no hard exit"),
        |mut person, peer| {
            let later =
                json!({"action": "schedule", "prompt": "check", "in": "2h", "name": "later"});
            to_a.send(call("jobs", later)).unwrap();
            to_a.send(says("Scheduled.")).unwrap();
            let lane = person.first();
            person.until("A waits", |app| app.ended(&lane) && app.ready());
            person.command(Command::Background);
            drop(person);
            std::thread::sleep(Duration::from_millis(500));
            let kept = peer.running();
            let schedule = protocol::Running {
                kind: "schedule".into(),
                name: "later".into(),
            };
            // An entry from the next interface reaches the lane: its prompt still waits.
            to_a.send(says("Again.")).unwrap();
            let again = protocol::Input {
                session: lane,
                text: "again".into(),
                ..protocol::Input::default()
            };
            peer.input(again);
            let deadline = Instant::now() + Duration::from_secs(5);
            while a.turns() < 3 && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            let turns = a.turns();
            peer.leave();
            assert_eq!(kept, [schedule], "its schedule runs on");
            assert_eq!(turns, 3, "the lane took the next interface's entry");
        },
    );
}

// An agent's run, as its own view in the interface shows it.

#[test]
fn what_an_agent_says_beside_its_calls_shows_in_its_own_view() {
    let mut window = Window::new(60, 20);
    let started = Event::Started {
        id: "s1".into(),
        name: "scout".into(),
    };
    window.event(started);
    for frame in said_by_agent("s1", "scout", "Checking the lockfile first.") {
        window.frame(frame);
    }
    let main = window.draw();
    assert!(!main.contains("Checking the lockfile"), "{main}");
    for code in [KeyCode::Down, KeyCode::Down, KeyCode::Enter] {
        window.press(code);
    }
    let scout = window.draw();
    assert!(scout.contains("Checking the lockfile first."), "{scout}");
}
