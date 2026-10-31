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

fn served() -> Served {
    served_by(None)
}

/// [`served`], as the daemon `host` when there is one.
#[expect(
    clippy::disallowed_methods,
    reason = "the in-process transport's channels, as the binary makes them"
)]
fn served_by(host: Option<Arc<Host>>) -> Served {
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
            host,
        };
        serve(None, &incoming, &theirs, &desk, &info);
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
fn an_interface_that_attaches_is_told_what_the_server_allows() {
    let served = served();
    served.call(
        1,
        Command::Initialize {
            version: protocol::VERSION,
            place: None,
            build: String::new(),
        },
    );
    assert_eq!(
        served.done(1),
        Done::Initialized {
            version: protocol::VERSION,
            max_lanes: 3,
            commands: vec![("help".into(), "the commands".into())],
            modes: vec![("plan".into(), "plan first".into())],
            background: false,
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

#[test]
fn the_place_an_interface_attaches_from_goes_to_the_desk() {
    let served = served();
    let place = protocol::Place {
        cwd: "/work/a".into(),
        env: vec![("VIRTUAL_ENV".into(), "/work/a/.venv".into())],
        ..protocol::Place::default()
    };
    let version = protocol::VERSION;
    let initialize = Command::Initialize {
        version,
        place: Some(place.clone()),
        build: String::new(),
    };
    served.call(1, initialize);
    assert!(matches!(served.done(1), Done::Initialized { .. }));
    assert_eq!(served.desk.recv_timeout(WAIT), Ok(Request::Attach(place)));
}

/// Another connection to the same sessions: where its frames go in, what comes out to it.
#[expect(
    clippy::disallowed_methods,
    reason = "the in-process transport's channels, as the binary makes them"
)]
fn another(served: &Served) -> (Sender<ToServer>, Receiver<ToClient>) {
    let (to_server, incoming) = channel();
    let (out, from_server) = channel();
    let (desk, _requests) = channel();
    let peer = Arc::clone(&served.peer);
    std::thread::spawn(move || {
        let info = Info {
            max_lanes: 3,
            commands: vec![],
            modes: vec![],
            host: None,
        };
        // A later interface joins as the daemon makes it: attached as its first frame comes.
        let Ok(first) = incoming.recv() else { return };
        rejoin(first, &incoming, &peer, out, &desk, &info);
    });
    (to_server, from_server)
}

/// What came of command `id` on a connection, and the frames before it.
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

/// A first command on a connection, which attaches it.
fn join(to_server: &Sender<ToServer>, id: u64) {
    let command = Command::Work;
    to_server.send(ToServer::Call { id, command }).unwrap();
}

#[test]
fn a_connection_that_attaches_later_gets_what_it_missed_then_what_waits_then_the_rest() {
    let served = served();
    let ui = Ui::new(Arc::clone(&served.peer)).for_lane("s1");
    ui.send(protocol::Event::Note("one".into()));
    ui.send(protocol::Event::Note("two".into()));
    let asker = ui.clone();
    let asking = std::thread::Builder::new().name("asking".into());
    let asking = asking
        .spawn(move || asker.approve("bash", "ls", &|| false))
        .unwrap();
    let id = loop {
        if let ToClient::Ask { id, .. } = served.out.recv_timeout(WAIT).unwrap() {
            break id;
        }
    };
    let (to_server, frames) = another(&served);
    join(&to_server, 1);
    let (before, done) = until_done(&frames, 1);
    assert!(matches!(done, Done::Work { .. }), "{done:?}");
    // Joining replayed everything the ring keeps, then the question waiting.
    assert_eq!(before.len(), 3, "{before:?}");
    assert!(matches!(&before[1], ToClient::Event { seq: Some(2), .. }));
    assert!(matches!(&before[2], ToClient::Ask { id: asked, .. } if *asked == id));
    // The first reply wins, from the later client; both are told it no longer waits.
    let reply = Reply::Approval(Approval::Always);
    to_server.send(ToServer::Reply { id, reply }).unwrap();
    assert_eq!(asking.join().unwrap(), Approval::Always);
    let late = Reply::Approval(Approval::No(None));
    served
        .to_server
        .send(ToServer::Reply { id, reply: late })
        .unwrap();
    let resolved = ToClient::Resolved { id };
    assert_eq!(frames.recv_timeout(WAIT).unwrap(), resolved);
    let first = served.out.recv_timeout(WAIT).unwrap();
    assert_eq!(first, resolved);
    ui.send(protocol::Event::Note("three".into()));
    for out in [&served.out, &frames] {
        let three = out.recv_timeout(WAIT).unwrap();
        assert!(
            matches!(three, ToClient::Event { seq: Some(3), .. }),
            "{three:?}"
        );
    }
}

#[test]
fn a_connection_is_told_only_what_came_of_its_own_commands() {
    let served = served();
    let (to_server, frames) = another(&served);
    join(&to_server, 1);
    assert!(matches!(until_done(&frames, 1).1, Done::Work { .. }));
    served.call(5, Command::Work);
    assert!(matches!(served.done(5), Done::Work { .. }));
    let call = ToServer::Call {
        id: 6,
        command: Command::Work,
    };
    to_server.send(call).unwrap();
    let (before, done) = until_done(&frames, 6);
    assert!(before.is_empty(), "{before:?}");
    assert!(matches!(done, Done::Work { .. }), "{done:?}");
}

#[test]
fn the_person_is_given_up_on_only_when_the_last_connection_is_gone() {
    let served = served();
    let peer = Arc::clone(&served.peer);
    let (to_server, frames) = another(&served);
    join(&to_server, 1);
    until_done(&frames, 1);
    open(&served, "s1");
    let first = Input {
        session: "s1".into(),
        text: "first".into(),
        ..Input::default()
    };
    served.call(9, Command::Input(first));
    served.done(9);
    drop(served);
    let deadline = std::time::Instant::now() + WAIT;
    while peer.hub.attached() > 1 {
        assert!(
            std::time::Instant::now() < deadline,
            "the first never detached"
        );
        std::thread::yield_now();
    }
    let entry = Input {
        session: "s1".into(),
        text: "still here".into(),
        ..Input::default()
    };
    let call = ToServer::Call {
        id: 2,
        command: Command::Input(entry),
    };
    to_server.send(call).unwrap();
    until_done(&frames, 2);
    assert_eq!(peer.inputs.next("s1").unwrap().text, "first");
    assert_eq!(peer.inputs.next("s1").unwrap().text, "still here");
    drop(to_server);
    assert_eq!(peer.inputs.next("s1"), None);
}

#[test]
fn moved_to_the_background_the_connections_end_leaves_the_prompts_waiting() {
    let served = served();
    served.call(1, Command::Background);
    assert_eq!(served.done(1), Done::Ok);
    let Served {
        desk,
        peer,
        to_server,
        ..
    } = served;
    // The desk is told, and then the connection is gone.
    drop(to_server);
    let asked: Vec<Request> = desk.iter().collect();
    assert_eq!(asked, [Request::Background]);
    peer.inputs.input(
        Input {
            session: "s1".into(),
            text: "from the next interface".into(),
            ..Input::default()
        },
        false,
    );
    let entry = peer.inputs.next("s1").map(|entry| entry.text);
    assert_eq!(entry.as_deref(), Some("from the next interface"));
}

#[test]
fn a_daemon_says_what_it_is_and_ends_when_asked() {
    let host = Host::new("0.4.0+a".into());
    let served = served_by(Some(Arc::clone(&host)));
    let _attached = host.attach();
    served.call(1, Command::Status);
    let Done::Status(protocol::Status {
        build,
        pid,
        attached,
        ..
    }) = served.done(1)
    else {
        panic!("no status");
    };
    assert_eq!(
        (build.as_str(), pid, attached),
        ("0.4.0+a", std::process::id(), 1)
    );
    served.call(2, Command::Shutdown);
    assert_eq!(served.done(2), Done::Ok);
    host.wait();
}

#[test]
fn a_daemon_refuses_a_client_of_another_build() {
    let served = served_by(Some(Host::new("0.4.0+a".into())));
    let hello = |build: &str| Command::Initialize {
        version: protocol::VERSION,
        place: None,
        build: build.into(),
    };
    served.call(1, hello("0.4.0+b"));
    assert!(matches!(served.done(1), Done::Failed { why } if why.contains("0.4.0+a")));
    served.call(2, hello("0.4.0+a"));
    assert!(matches!(served.done(2), Done::Initialized { .. }));
}

#[test]
fn in_process_there_is_no_daemon_to_ask_or_stop() {
    let served = served();
    for (id, command) in [(1, Command::Status), (2, Command::Shutdown)] {
        served.call(id, command);
        assert!(matches!(served.done(id), Done::Failed { .. }));
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

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "the in-process transport's channels, as the binary makes them"
)]
fn moved_to_the_background_once_the_desk_ended_keeps_nothing() {
    let (to_server, incoming) = channel();
    let (out, _from_server) = channel();
    let (desk, requests) = channel();
    drop(requests);
    let peer = Peer::new(out);
    let background = ToServer::Call {
        id: 1,
        command: Command::Background,
    };
    to_server.send(background).unwrap();
    drop(to_server);
    let info = Info {
        max_lanes: 1,
        commands: vec![],
        modes: vec![],
        host: None,
    };
    assert!(!serve(None, &incoming, &peer, &desk, &info));
}
