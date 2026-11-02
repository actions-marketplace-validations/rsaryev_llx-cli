use super::*;
use std::sync::mpsc::{self, Receiver, Sender};

use protocol::{Command, ToServer};
use std::time::Duration;

use crate::testing::fake_ui;

#[test]
fn a_lanes_ui_tags_what_it_sends() {
    let (ui, frames) = fake_ui();
    ui.for_lane("a").send(Event::Note("hi".into()));
    let frame = frames.try_recv().unwrap();
    let note = ToClient::Event {
        session: Some("a".into()),
        event: Event::Note("hi".into()),
        seq: Some(1),
    };
    assert_eq!(frame, note);
}

#[test]
fn an_agents_events_are_in_its_own_name() {
    let (ui, frames) = fake_ui();
    ui.for_lane("a")
        .for_lane("agent")
        .send(Event::Answer("found".into()));
    let frame = frames.try_recv().unwrap();
    let answer = ToClient::Event {
        session: Some("agent".into()),
        event: Event::Answer("found".into()),
        seq: Some(1),
    };
    assert_eq!(frame, answer);
}

#[test]
fn the_servers_own_ui_tags_nothing() {
    // What the server says of itself (the end of the work) belongs to no session.
    let (ui, frames) = fake_ui();
    ui.send(Event::Quit);
    let quit = ToClient::Event {
        session: None,
        event: Event::Quit,
        seq: None,
    };
    assert_eq!(frames.try_recv().unwrap(), quit);
}

const WAIT: Duration = Duration::from_secs(5);

/// A server for `peer` on a thread of its own, as the binary serves the first interface: the
/// frames it is sent go in through the sender, and it ends when the sender is dropped.
#[expect(
    clippy::disallowed_methods,
    reason = "the in-process transport's channels, as the binary makes them"
)]
fn serving(peer: &Arc<Peer>) -> Sender<ToServer> {
    let (to_server, incoming) = mpsc::channel();
    let (desk, _requests) = mpsc::channel();
    let peer = Arc::clone(peer);
    let serving = std::thread::Builder::new().name("serving".into());
    serving
        .spawn(move || {
            let info = super::super::serve::Info {
                max_lanes: 3,
                commands: vec![],
                modes: vec![],
                host: None,
            };
            super::super::serve::serve(None, &incoming, &peer, &desk, &info);
        })
        .unwrap();
    to_server
}

/// `ask` on a thread of its own; its answer comes through the receiver.
fn asking<T: Send + 'static>(ask: impl FnOnce() -> T + Send + 'static) -> Receiver<T> {
    let (answer, answered) = sync_channel(1);
    let thread = std::thread::Builder::new().name("asking".into());
    thread
        .spawn(move || {
            let _ = answer.send(ask());
        })
        .unwrap();
    answered
}

/// The id of the next question sent.
fn asked(frames: &Receiver<ToClient>) -> u64 {
    loop {
        if let ToClient::Ask { id, .. } = frames.recv_timeout(WAIT).unwrap() {
            return id;
        }
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
fn resolved(frames: &Receiver<ToClient>, id: u64) {
    loop {
        match frames.recv_timeout(WAIT).unwrap() {
            ToClient::Resolved { id: of } if of == id => return,
            _ => {}
        }
    }
}

fn pick(ui: &Ui) -> Receiver<Option<usize>> {
    let mut ui = ui.clone();
    asking(move || ui.pick("Provider", &[Item::new("a", "", false)]))
}

#[test]
fn a_choice_waiting_when_the_person_leaves_is_withdrawn() {
    // `/login`'s list has no task to stop it: the person leaving must end it, or the lane waits
    // forever.
    let (ui, peer, frames) = crate::testing::fake_peer();
    let to_server = serving(&peer);
    let picked = pick(&ui.for_lane("s1"));
    let id = asked(&frames);
    let leave = ToServer::Call {
        id: 1,
        command: Command::Leave,
    };
    to_server.send(leave).unwrap();
    assert_eq!(picked.recv_timeout(WAIT).unwrap(), None);
    resolved(&frames, id);
}

#[test]
fn a_line_waiting_when_the_last_interface_is_gone_is_withdrawn() {
    let (ui, peer, frames) = crate::testing::fake_peer();
    let to_server = serving(&peer);
    let mut lane = ui.for_lane("s1");
    let typed = asking(move || lane.secret("Key"));
    // Nobody is left to tell it was withdrawn: only that it ends.
    asked(&frames);
    drop(to_server);
    assert_eq!(typed.recv_timeout(WAIT).unwrap(), None);
}

#[test]
fn closing_a_lane_withdraws_its_questions_and_leaves_the_others() {
    let (ui, peer, frames) = crate::testing::fake_peer();
    let to_server = serving(&peer);
    // The desk opened both lanes: only an open lane is closed.
    for id in ["s1", "s2"] {
        let (id, name) = (id.to_string(), id.to_string());
        ui.send(Event::LaneStarted { id, name });
    }
    let first = pick(&ui.for_lane("s1"));
    let one = asked(&frames);
    let second = pick(&ui.for_lane("s2"));
    let two = asked(&frames);
    let close = ToServer::Call {
        id: 1,
        command: Command::Close {
            session: "s1".into(),
        },
    };
    to_server.send(close).unwrap();
    assert_eq!(first.recv_timeout(WAIT).unwrap(), None);
    resolved(&frames, one);
    assert!(second.recv_timeout(Duration::from_millis(200)).is_err());
    let reply = ToServer::Reply {
        id: two,
        reply: Reply::Choice(Some(0)),
    };
    to_server.send(reply).unwrap();
    assert_eq!(second.recv_timeout(WAIT).unwrap(), Some(0));
}

#[test]
fn a_question_waiting_does_not_keep_asking_whether_it_was_stopped() {
    // A question may wait for hours in the daemon: it sleeps until something changes.
    let (ui, peer, frames) = crate::testing::fake_peer();
    let checks = Arc::new(AtomicU64::new(0));
    let counted = Arc::clone(&checks);
    let lane = ui.for_lane("s1");
    let approved = asking(move || {
        lane.approve("bash", "ls", &|| {
            counted.fetch_add(1, Ordering::SeqCst);
            false
        })
    });
    let id = asked(&frames);
    let rung = || BELL.rung();
    let before = rung();
    assert!(approved.recv_timeout(Duration::from_millis(1500)).is_err());
    // The other tests ring the bell too: each ring may look once, and so may each second.
    let rings = rung().wrapping_sub(before);
    let checked = checks.load(Ordering::SeqCst);
    assert!(
        checked <= rings + 3,
        "checked {checked} times in 1.5 s, {rings} rings"
    );
    peer.replied(id, Reply::Approval(Approval::Yes));
    assert_eq!(approved.recv_timeout(WAIT).unwrap(), Approval::Yes);
}

#[test]
fn a_stop_by_the_person_withdraws_the_question_at_once() {
    let (ui, peer, frames) = crate::testing::fake_peer();
    let to_server = serving(&peer);
    let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = Arc::clone(&stopped);
    let _known = crate::interrupt::register("ui-stop-lane", move || {
        flag.store(true, Ordering::SeqCst);
        false
    });
    let lane = ui.for_lane("ui-stop-lane");
    let seen = Arc::clone(&stopped);
    let approved = asking(move || lane.approve("bash", "ls", &|| seen.load(Ordering::SeqCst)));
    let id = asked(&frames);
    // Past the first look, so only being woken can withdraw it in time.
    assert!(approved.recv_timeout(Duration::from_millis(50)).is_err());
    let interrupt = ToServer::Call {
        id: 1,
        command: Command::Interrupt {
            session: "ui-stop-lane".into(),
        },
    };
    let at = std::time::Instant::now();
    to_server.send(interrupt).unwrap();
    assert_eq!(approved.recv_timeout(WAIT).unwrap(), Approval::No(None));
    assert!(
        at.elapsed() < Duration::from_millis(500),
        "{:?}",
        at.elapsed()
    );
    resolved(&frames, id);
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "a client attached later, as the socket attaches one"
)]
fn a_question_withdrawn_as_it_is_asked_is_never_replayed() {
    // The withdrawal may come between the question's entry and its frame: what a client that
    // attaches later is given must still say the question is over.
    for _ in 0..5000 {
        let (ui, peer, _frames) = crate::testing::fake_peer();
        let picked = pick(&ui.for_lane("s1"));
        while picked.try_recv().is_err() {
            peer.abandon(None);
        }
        let (out, later) = mpsc::channel();
        peer.hub.attach(out);
        let stale = later
            .try_iter()
            .any(|frame| matches!(frame, ToClient::Ask { .. }));
        assert!(!stale, "a withdrawn question was replayed");
    }
}
