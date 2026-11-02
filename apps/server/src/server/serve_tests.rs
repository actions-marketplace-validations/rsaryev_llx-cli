use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use protocol::{Approval, Asked, Input, Reply, Target};

use super::*;
use crate::server::Ui;

const WAIT: Duration = Duration::from_secs(5);

/// A server on a thread of its own: where frames go in, what comes out, what the desk is asked.
struct Served {
    to_server: Sender<ToServer>,
    out: Receiver<ToClient>,
    desk: Receiver<Request>,
    peer: Arc<Peer>,
}

#[expect(
    clippy::disallowed_methods,
    reason = "the seam's channels, as the binary makes them"
)]
fn served() -> Served {
    let (to_server, incoming) = channel();
    let (out, from_server) = channel();
    let (desk, requests) = channel();
    let peer = Peer::new(out);
    let theirs = Arc::clone(&peer);
    std::thread::spawn(move || {
        let info = Info {
            max_lanes: 3,
            commands: vec![("help".into(), "the commands".into())],
            modes: vec![("plan".into(), "plan first".into())],
        };
        serve(&incoming, &theirs, &desk, &info);
    });
    Served {
        to_server,
        out: from_server,
        desk: requests,
        peer,
    }
}

impl Served {
    fn call(&self, id: u64, command: Command) {
        self.to_server.send(ToServer::Call { id, command }).unwrap();
    }

    /// What came of command `id`, past the events before it.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "a test picks out the variant it checks"
    )]
    fn done(&self, id: u64) -> Done {
        loop {
            match self.out.recv_timeout(WAIT).unwrap() {
                ToClient::Done { id: of, done } if of == id => return done,
                _ => {}
            }
        }
    }
}

#[test]
fn an_interface_that_starts_is_told_what_the_server_allows() {
    let served = served();
    served.call(1, Command::Initialize);
    assert_eq!(
        served.done(1),
        Done::Initialized {
            max_lanes: 3,
            commands: vec![("help".into(), "the commands".into())],
            modes: vec![("plan".into(), "plan first".into())],
        }
    );
    let ui = Ui::new(Arc::clone(&served.peer));
    let (id, name) = ("s1".to_string(), "first".to_string());
    ui.send(protocol::Event::LaneStarted { id, name });
    ui.for_lane("s1")
        .send(protocol::Event::TaskStarted("fix it".into()));
    served.call(2, Command::Work);
    let running = |kind: &str, name: &str| protocol::Running {
        kind: kind.into(),
        name: name.into(),
    };
    let running = vec![running("session", "first")];
    assert_eq!(served.done(2), Done::Work { running });
}

/// Lane `id` open, as the desk says it.
fn open(served: &Served, id: &str) {
    let (id, name) = (id.to_string(), "first".to_string());
    Ui::new(Arc::clone(&served.peer)).send(protocol::Event::LaneStarted { id, name });
}

#[test]
fn what_the_person_types_is_the_lanes_next_entry() {
    let served = served();
    open(&served, "s1");
    let input = Input {
        session: "s1".into(),
        text: "fix it".into(),
        ..Input::default()
    };
    served.call(1, Command::Input(input));
    assert_eq!(served.done(1), Done::Ok);
    let entry = served.peer.inputs.next("s1").unwrap();
    assert_eq!(entry.text, "fix it");
}

#[test]
fn words_typed_while_the_lanes_task_runs_are_for_the_task() {
    let served = served();
    open(&served, "s1");
    let lane = Ui::new(Arc::clone(&served.peer)).for_lane("s1");
    lane.send(protocol::Event::TaskStarted("fix it".into()));
    let input = Input {
        session: "s1".into(),
        text: "use the cache".into(),
        ..Input::default()
    };
    served.call(1, Command::Input(input));
    assert_eq!(served.done(1), Done::Ok);
    assert_eq!(lane.steered(), ["use the cache"]);
}

#[test]
fn a_reply_reaches_the_question_waiting_under_its_id() {
    let served = served();
    let ui = Ui::new(Arc::clone(&served.peer)).for_lane("s1");
    let asking = std::thread::Builder::new().name("asking".into());
    let asking = asking
        .spawn(move || ui.approve("bash", "ls", &|| false))
        .unwrap();
    let id = loop {
        if let ToClient::Ask { id, session, asked } = served.out.recv_timeout(WAIT).unwrap() {
            assert_eq!(session, "s1");
            assert!(matches!(asked, Asked::Approve { .. }));
            break id;
        }
    };
    let reply = Reply::Approval(Approval::Always);
    served
        .to_server
        .send(ToServer::Reply { id, reply })
        .unwrap();
    assert_eq!(asking.join().unwrap(), Approval::Always);
}

#[test]
fn a_question_whose_task_stopped_is_withdrawn() {
    let served = served();
    let ui = Ui::new(Arc::clone(&served.peer)).for_lane("s1");
    assert_eq!(ui.approve("bash", "ls", &|| true), Approval::No(None));
    let asked = served.out.recv_timeout(WAIT).unwrap();
    let ToClient::Ask { id, .. } = asked else {
        panic!("{asked:?}");
    };
    assert_eq!(
        served.out.recv_timeout(WAIT).unwrap(),
        ToClient::Resolved { id }
    );
}

#[test]
fn opening_listing_and_quitting_are_the_desks() {
    let served = served();
    served.call(
        1,
        Command::Open {
            target: Target::New,
        },
    );
    served.call(2, Command::List);
    served.call(3, Command::QuitAll);
    assert_eq!(served.done(3), Done::Ok);
    let asked: Vec<Request> = served.desk.try_iter().collect();
    assert_eq!(
        asked,
        [Request::Open(Target::New), Request::List, Request::Quit]
    );
}

#[test]
fn once_the_interface_is_gone_nothing_waits_for_the_person() {
    let served = served();
    let peer = Arc::clone(&served.peer);
    drop(served);
    assert_eq!(peer.inputs.next("s1"), None);
}

/// What came of command `id`, and the frames before it.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
fn until_done(frames: &Receiver<ToClient>, id: u64) -> (Vec<ToClient>, Done) {
    let mut before = vec![];
    loop {
        match frames.recv_timeout(WAIT).unwrap() {
            ToClient::Done { id: of, done } if of == id => return (before, done),
            frame => before.push(frame),
        }
    }
}
#[test]
fn what_is_typed_for_a_session_not_open_is_refused_and_kept_nowhere() {
    let served = served();
    let input = Input {
        session: "gone".into(),
        text: "fix it".into(),
        ..Input::default()
    };
    served.call(1, Command::Input(input));
    assert!(matches!(served.done(1), Done::Failed { why } if why.contains("gone")));
    served.call(
        2,
        Command::Close {
            session: "gone".into(),
        },
    );
    assert!(matches!(served.done(2), Done::Failed { .. }));
}

/// A lane's end forgets its queue first, then what runs lets the lane go (`Ends`): what the
/// person types in between is refused, never kept in a queue made again for a lane gone.
#[test]
fn what_is_typed_for_a_lane_that_is_ending_is_refused_and_kept_nowhere() {
    let served = served();
    open(&served, "s1");
    // The lane's end, half done: its queue forgotten, its work not let go yet.
    served.peer.inputs.forget("s1");
    let input = Input {
        session: "s1".into(),
        text: "fix it".into(),
        ..Input::default()
    };
    served.call(1, Command::Input(input));
    let (told, done) = until_done(&served.out, 1);
    assert!(
        matches!(&done, Done::Failed { why } if why.contains("s1")),
        "{done:?}"
    );
    let ended = told.iter().position(|frame| {
        matches!(
            frame,
            ToClient::Event {
                event: protocol::Event::LaneEnded,
                ..
            }
        )
    });
    let after = &told[ended.map_or(0, |at| at + 1)..];
    let queued = after.iter().any(|frame| {
        matches!(
            frame,
            ToClient::Event {
                event: protocol::Event::Queue { .. },
                ..
            }
        )
    });
    assert!(!queued, "a queue told for a lane gone: {told:?}");
}
