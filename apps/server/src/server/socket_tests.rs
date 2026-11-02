use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::time::{Duration, Instant};

use protocol::{Approval, Asked, Command, Done, Event, Input, Reply};

use super::*;
use crate::desk::Request;
use crate::server::serve::{Info, serve};
use crate::server::{Peer, Ui};

const WAIT: Duration = Duration::from_secs(5);

/// A socket path in a directory of its own (made by `bind`), short enough for `sun_path`.
pub(crate) fn place(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("llx-s{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("d.sock")
}

/// A daemon at `path` serving each connection as the binary serves its interface: its peer
/// handed to the test, and a word when its `serve` returns.
pub(crate) struct Daemon {
    pub(crate) peers: Receiver<Arc<Peer>>,
    pub(crate) ended: Receiver<()>,
}

#[expect(
    clippy::disallowed_methods,
    reason = "the desk's requests, as the binary makes its channel"
)]
pub(crate) fn daemon(path: &Path) -> Daemon {
    let (peers, peers_out) = sync_channel(4);
    let (ended, ended_out) = sync_channel(4);
    let peers: SyncSender<Arc<Peer>> = peers;
    accept(bind(path).unwrap(), move |connection| {
        let (desk, _requests) = std::sync::mpsc::channel::<Request>();
        let peer = Peer::new(connection.out);
        let _ = peers.send(Arc::clone(&peer));
        let info = Info {
            max_lanes: 3,
            commands: vec![],
            modes: vec![],
            host: None,
        };
        serve(None, &connection.incoming, &peer, &desk, &info);
        let _ = ended.send(());
    })
    .unwrap();
    Daemon {
        peers: peers_out,
        ended: ended_out,
    }
}

#[test]
fn the_socket_is_the_persons_alone_and_a_stale_one_is_replaced() {
    let path = place("modes");
    drop(bind(&path).unwrap());
    let _listener = bind(&path).unwrap();
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(path.parent().unwrap()), 0o700);
    assert_eq!(mode(&path), 0o600);
}

/// A connection to the socket as an interface's: frames written as lines, and read back.
struct Connected {
    to: UnixStream,
    from: std::io::Lines<BufReader<UnixStream>>,
}

impl Connected {
    fn to(path: &Path) -> Self {
        let to = UnixStream::connect(path).unwrap();
        to.set_read_timeout(Some(WAIT)).unwrap();
        let from = BufReader::new(to.try_clone().unwrap()).lines();
        Self { to, from }
    }

    fn send(&mut self, frame: &ToServer) {
        let mut line = serde_json::to_vec(frame).unwrap();
        line.push(b'\n');
        self.to.write_all(&line).unwrap();
    }

    fn next(&mut self) -> ToClient {
        serde_json::from_str(&self.from.next().unwrap().unwrap()).unwrap()
    }

    /// `command` called under `id`, and what it came to (the events before it passed over).
    fn call(&mut self, id: u64, command: Command) -> Done {
        self.send(&ToServer::Call { id, command });
        loop {
            if let ToClient::Done { id: answered, done } = self.next() {
                assert_eq!(answered, id);
                return done;
            }
        }
    }
}

#[test]
fn frames_over_the_socket_are_served_as_in_process() {
    let path = place("proof");
    let daemon = daemon(&path);
    let mut client = Connected::to(&path);
    let handshake = Command::Initialize {
        version: protocol::VERSION,
        place: None,
        build: String::new(),
    };
    assert_eq!(
        client.call(1, handshake),
        Done::Initialized {
            version: protocol::VERSION,
            max_lanes: 3,
            commands: vec![],
            modes: vec![],
            background: false,
        }
    );
    let peer = daemon.peers.recv_timeout(WAIT).unwrap();
    let (id, name) = ("s1".to_string(), "first".to_string());
    Ui::new(Arc::clone(&peer)).send(Event::LaneStarted { id, name });

    let input = Input {
        session: "s1".into(),
        text: "fix it".into(),
        ..Input::default()
    };
    assert_eq!(client.call(2, Command::Input(input)), Done::Ok);
    assert_eq!(peer.inputs.next("s1").unwrap().text, "fix it");

    let ui = Ui::new(Arc::clone(&peer)).for_lane("s1");
    let asking = std::thread::Builder::new().name("asking".into());
    let asking = asking
        .spawn(move || ui.approve("bash", "ls", &|| false))
        .unwrap();
    let id = loop {
        if let ToClient::Ask { id, session, asked } = client.next() {
            assert_eq!(session, "s1");
            assert!(matches!(asked, Asked::Approve { .. }));
            break id;
        }
    };
    let reply = Reply::Approval(Approval::Always);
    client.send(&ToServer::Reply { id, reply });
    assert_eq!(asking.join().unwrap(), Approval::Always);

    drop(client);
    daemon.ended.recv_timeout(WAIT).unwrap();
}

#[test]
fn a_line_that_is_not_a_frame_is_dropped_and_said() {
    let path = place("malformed");
    let _daemon = daemon(&path);
    let mut stream = UnixStream::connect(&path).unwrap();
    stream.write_all(b"{not json\n").unwrap();
    let call = ToServer::Call {
        id: 7,
        command: Command::Work,
    };
    let mut line = serde_json::to_vec(&call).unwrap();
    line.push(b'\n');
    stream.write_all(&line).unwrap();
    stream.set_read_timeout(Some(WAIT)).unwrap();
    let frames: Vec<ToClient> = BufReader::new(stream)
        .lines()
        .take(2)
        .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
        .collect();
    assert!(
        matches!(&frames[0], ToClient::Event { event: Event::Note(note), .. } if note.contains("dropped")),
        "{frames:?}"
    );
    let done = Done::Work { running: vec![] };
    assert_eq!(frames[1], ToClient::Done { id: 7, done });
}

#[test]
fn a_client_that_never_reads_is_cut_off_and_never_waited_for() {
    let path = place("slow");
    let daemon = daemon(&path);
    let _stream = UnixStream::connect(&path).unwrap();
    let peer = daemon.peers.recv_timeout(WAIT).unwrap();
    let started = Instant::now();
    for _ in 0..20_000 {
        peer.send(ToClient::Event {
            session: None,
            event: Event::Note("x".repeat(1_000)),
            seq: None,
        });
    }
    assert!(started.elapsed() < WAIT, "the server waited on the client");
    daemon.ended.recv_timeout(WAIT).unwrap();
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "the first client's channel, read by no one; the test's late reader's pause"
)]
fn a_replay_longer_than_the_queue_never_cuts_the_client() {
    let (first, _unread) = std::sync::mpsc::channel();
    let peer = Peer::new(first);
    let kept = crate::server::hub::KEPT;
    for at in 0..kept + 500 {
        peer.send(ToClient::Event {
            session: Some("s1".into()),
            event: Event::Note(format!("{at:0>100}")),
            seq: None,
        });
    }
    let (ours, theirs) = UnixStream::pair().unwrap();
    ours.set_read_timeout(Some(WAIT)).unwrap();
    let connection = connection(theirs).unwrap();
    peer.hub.attach(connection.out);
    // The client reads only once the whole replay is posted, as a terminal busy drawing does.
    std::thread::sleep(Duration::from_millis(300));
    let mut lines = BufReader::new(ours).lines();
    let replayed = (lines.by_ref().take(kept + 1))
        .filter(|line| {
            line.as_ref()
                .is_ok_and(|line| line.contains("session/event"))
        })
        .count();
    assert_eq!(replayed, kept, "the whole ring, after its gap");
    peer.send(ToClient::Event {
        session: None,
        event: Event::Note("still here".into()),
        seq: None,
    });
    let live = lines.next().unwrap().unwrap();
    assert!(live.contains("still here"), "{live}");
}

#[test]
fn a_call_that_is_not_text_is_answered_failed_and_the_connection_goes_on() {
    let path = place("bytes");
    let _daemon = daemon(&path);
    let mut client = Connected::to(&path);
    // What the person typed, with a byte in it that no UTF-8 text has.
    let typed = Input {
        session: "s1".into(),
        text: "~".into(),
        ..Input::default()
    };
    let call = ToServer::Call {
        id: 9,
        command: Command::Input(typed),
    };
    let mut line = serde_json::to_vec(&call).unwrap();
    for byte in line.iter_mut().filter(|byte| **byte == b'~') {
        *byte = 0xff;
    }
    line.push(b'\n');
    client.to.write_all(&line).unwrap();
    let failed = client.next();
    assert!(
        matches!(
            &failed,
            ToClient::Done {
                id: 9,
                done: Done::Failed { .. }
            }
        ),
        "{failed:?}"
    );
    let work = Done::Work { running: vec![] };
    assert_eq!(client.call(7, Command::Work), work);
}

#[test]
fn a_call_the_server_cannot_read_is_answered_failed_under_its_id() {
    let path = place("unknown");
    let _daemon = daemon(&path);
    let mut stream = UnixStream::connect(&path).unwrap();
    let call = r#"{"method":"command/call","params":{"id":9,"command":{"type":"fly"}}}"#;
    stream.write_all(format!("{call}\n").as_bytes()).unwrap();
    stream.set_read_timeout(Some(WAIT)).unwrap();
    let line = BufReader::new(stream).lines().next().unwrap().unwrap();
    let frame: ToClient = serde_json::from_str(&line).unwrap();
    assert!(
        matches!(&frame, ToClient::Done { id: 9, done: Done::Failed { why } } if why.contains("fly")),
        "{frame:?}"
    );
}

#[test]
fn an_accept_that_keeps_failing_backs_off_and_a_success_starts_again() {
    let error = || Err(std::io::Error::other("too many open files"));
    let (ours, theirs) = UnixStream::pair().unwrap();
    drop(ours);
    let incoming = vec![error(), error(), error(), Ok(theirs), error()];
    let paused = std::sync::Mutex::new(vec![]);
    let served = std::sync::atomic::AtomicUsize::new(0);
    accepting(
        incoming,
        |_stream| {
            served.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        },
        |pause| paused.lock().unwrap().push(pause),
    );
    let paused = paused.into_inner().unwrap();
    assert_eq!(served.into_inner(), 1);
    assert_eq!(paused.len(), 4, "{paused:?}");
    assert!(paused[0] < paused[1] && paused[1] < paused[2], "{paused:?}");
    assert_eq!(paused[3], paused[0], "{paused:?}");
}
