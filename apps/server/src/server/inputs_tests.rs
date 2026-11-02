use std::sync::Arc;
use std::time::Duration;

use super::*;

/// What the interface was told, each event with its lane.
type Told = Arc<Mutex<Vec<(String, Event)>>>;

/// Inputs that keep what they told the interface, with lane `s1` open.
fn inputs() -> (Arc<Inputs>, Told) {
    let told = Arc::new(Mutex::new(vec![]));
    let kept = Arc::clone(&told);
    let inputs = Inputs::new(move |lane, event| kept.lock().unwrap().push((lane.into(), event)));
    inputs.open("s1");
    (Arc::new(inputs), told)
}

fn sent(text: &str) -> Input {
    Input {
        session: "s1".into(),
        text: text.into(),
        ..Input::default()
    }
}

/// The lane's queue as last shown: ready, steered, queued, background.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
fn last_queue(told: &Mutex<Vec<(String, Event)>>) -> (bool, Vec<String>, Vec<String>, Vec<String>) {
    let told = told.lock().unwrap();
    let queue = told.iter().rev().find_map(|(_, event)| match event {
        Event::Queue {
            ready,
            steered,
            queued,
            background,
        } => Some((*ready, steered.clone(), queued.clone(), background.clone())),
        _ => None,
    });
    queue.unwrap()
}

/// Lane `lane` at its prompt: `next` waits on a thread of its own until an entry comes.
#[expect(
    clippy::disallowed_methods,
    reason = "a test's poll: the lane says it waits only through its queue"
)]
fn waiting_in(inputs: &Arc<Inputs>, lane: &'static str) -> std::thread::JoinHandle<Option<Typed>> {
    let theirs = Arc::clone(inputs);
    let thread = std::thread::Builder::new().name("prompt".into());
    let prompt = thread.spawn(move || theirs.next(lane)).unwrap();
    while !inputs
        .lock()
        .lanes
        .get(lane)
        .is_some_and(|state| state.ready)
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    prompt
}

fn waiting(inputs: &Arc<Inputs>) -> std::thread::JoinHandle<Option<Typed>> {
    waiting_in(inputs, "s1")
}

#[test]
fn what_is_sent_to_a_lane_at_its_prompt_is_its_next_entry() {
    let (inputs, told) = inputs();
    let prompt = waiting(&inputs);
    assert!(last_queue(&told).0, "the interface is told the lane waits");
    inputs.input(sent("fix the parser"), false);
    assert_eq!(prompt.join().unwrap(), Some(Typed::from("fix the parser")));
    assert_eq!(last_queue(&told), (false, vec![], vec![], vec![]));
}

/// The interface is told the queue in the order it changed: an entry the lane took while the
/// queue that held it was still being told is never shown as waiting last.
#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "a test's poll: the telling is held until the prompt took the entry"
)]
fn an_entry_taken_while_its_queue_is_told_is_not_left_shown() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let taken = Arc::new(AtomicBool::new(false));
    let told: Told = Arc::new(Mutex::new(vec![]));
    let (kept, held) = (Arc::clone(&told), Arc::clone(&taken));
    let inputs = Arc::new(Inputs::new(move |lane, event| {
        let holds_it = matches!(&event, Event::Queue { queued, .. } if !queued.is_empty());
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while holds_it && !held.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        kept.lock().unwrap().push((lane.into(), event));
    }));
    let (theirs, done) = (Arc::clone(&inputs), Arc::clone(&taken));
    let thread = std::thread::Builder::new().name("prompt".into());
    let prompt = thread
        .spawn(move || {
            let got = theirs.next("s1");
            done.store(true, Ordering::SeqCst);
            got
        })
        .unwrap();
    while !inputs
        .lock()
        .lanes
        .get("s1")
        .is_some_and(|state| state.ready)
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    inputs.input(sent("fix the parser"), false);
    assert_eq!(prompt.join().unwrap(), Some(Typed::from("fix the parser")));
    assert_eq!(last_queue(&told), (false, vec![], vec![], vec![]));
}

#[test]
fn words_sent_while_a_task_works_go_to_it_and_are_shown_as_theirs_when_read() {
    let (inputs, told) = inputs();
    inputs.input(sent("use the cache"), true);
    assert_eq!(last_queue(&told).1, ["use the cache"]);
    assert!(inputs.steering("s1"), "a look leaves them");
    assert_eq!(inputs.steered("s1"), ["use the cache"]);
    assert!(!inputs.steering("s1"));
    assert!(
        told.lock()
            .unwrap()
            .iter()
            .any(|(_, e)| *e == Event::Steered(vec!["use the cache".into()]))
    );
    assert!(inputs.steered("s1").is_empty());
}

#[test]
fn a_command_a_picture_or_tab_waits_for_the_task_to_end() {
    let (inputs, told) = inputs();
    inputs.input(sent("/diff"), true);
    inputs.input(
        Input {
            images: vec!["/tmp/a.png".into()],
            ..sent("look")
        },
        true,
    );
    inputs.input(
        Input {
            later: true,
            ..sent("then the docs")
        },
        true,
    );
    let (_, steered, queued, _) = last_queue(&told);
    assert!(steered.is_empty());
    assert_eq!(queued, ["/diff", "look", "then the docs"]);
}

#[test]
fn what_the_task_did_not_read_is_the_next_thing_it_is_given() {
    let (inputs, _told) = inputs();
    inputs.input(sent("/diff"), true);
    inputs.input(sent("and faster"), true);
    assert_eq!(inputs.next("s1"), Some(Typed::from("and faster")));
    assert_eq!(inputs.next("s1"), Some(Typed::from("/diff")));
}

#[test]
fn words_sent_while_no_task_runs_wait_for_the_prompt() {
    let (inputs, told) = inputs();
    inputs.input(sent("next, the docs"), false);
    let (_, steered, queued, _) = last_queue(&told);
    assert!(steered.is_empty(), "no task to steer: {steered:?}");
    assert_eq!(queued, ["next, the docs"]);
}

#[test]
fn the_background_waits_behind_the_person_and_reads_as_where_it_came_from() {
    let (inputs, told) = inputs();
    inputs.background(
        "s1",
        "<monitor job=\"ready\" ended=\"met after 1 check (0 s)\">\nup\n</monitor>",
    );
    inputs.input(
        Input {
            later: true,
            ..sent("mine first")
        },
        false,
    );
    assert_eq!(last_queue(&told).3, ["≋ ready · met after 1 check (0 s)"]);
    assert_eq!(inputs.next("s1"), Some(Typed::from("mine first")));
    assert!(
        inputs
            .next("s1")
            .is_some_and(|entry| entry.text.starts_with("<monitor"))
    );
}

#[test]
fn a_closed_lane_and_a_person_who_left_get_nothing_more() {
    let (inputs, _told) = inputs();
    let prompt = waiting(&inputs);
    inputs.close("s1");
    assert_eq!(prompt.join().unwrap(), None);
    let prompt = waiting_in(&inputs, "s2");
    inputs.leave();
    assert_eq!(prompt.join().unwrap(), None);
}

#[test]
fn reading_nothing_new_tells_the_interface_nothing() {
    let (inputs, told) = inputs();
    inputs.input(sent("use the cache"), true);
    let before = told.lock().unwrap().len();
    for _ in 0..5 {
        assert!(inputs.steered("s1").len() <= 1);
    }
    let after: Vec<Event> = (told.lock().unwrap().iter().skip(before))
        .map(|(_, event)| event.clone())
        .collect();
    let queues = after
        .iter()
        .filter(|event| matches!(event, Event::Queue { .. }));
    assert_eq!(queues.count(), 1, "one change, told once: {after:?}");
}

#[test]
fn a_lane_that_ended_is_said_ended_and_closing_it_keeps_nothing() {
    let (inputs, told) = inputs();
    inputs.input(sent("first"), false);
    inputs.forget("s1");
    assert_eq!(
        told.lock().unwrap().last(),
        Some(&("s1".to_string(), Event::LaneEnded))
    );
    inputs.close("s1");
    assert!(
        inputs.lock().lanes.is_empty(),
        "nothing kept for a lane gone"
    );
}

/// What comes for a lane once its end forgot it (typed as it ended, a monitor's last lines) is
/// refused: no queue is made again for a lane gone, nor told to the interface after its end.
#[test]
fn what_comes_for_a_lane_once_it_is_forgotten_is_kept_nowhere() {
    let (inputs, told) = inputs();
    inputs.input(sent("first"), false);
    inputs.forget("s1");
    assert!(!inputs.input(sent("typed as it ended"), true), "refused");
    inputs.background("s1", "<monitor job=\"ticks\">\ntick 9\n</monitor>");
    assert_eq!(
        told.lock().unwrap().last(),
        Some(&("s1".to_string(), Event::LaneEnded))
    );
    assert!(
        inputs.lock().lanes.is_empty(),
        "nothing kept for a lane gone"
    );
}

#[test]
fn a_prompt_that_comes_after_the_person_left_gets_nothing() {
    let (inputs, _told) = inputs();
    inputs.leave();
    assert_eq!(
        inputs.next("late"),
        None,
        "a lane that first waits after the leave"
    );
}
