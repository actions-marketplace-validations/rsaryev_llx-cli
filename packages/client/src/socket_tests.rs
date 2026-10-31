use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use protocol::{Command, Done, Event, ToClient, ToServer};

use crate::{CallError, Client};

const WAIT: Duration = Duration::from_secs(5);

/// A socket in a directory of its own, short enough for `sun_path`.
fn place(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("llx-c{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("s.sock")
}

/// A client connected to a bare listener, the server's end of it, and what reaches the interface.
#[expect(
    clippy::disallowed_methods,
    reason = "what the test's interface is shown, as many frames as the test sends"
)]
fn connected(name: &str) -> (Client, UnixStream, Receiver<ToClient>) {
    let path = place(name);
    let listener = UnixListener::bind(&path).unwrap();
    let (shown, seen) = channel();
    let client = Client::connect(&path, move |frame| {
        let _ = shown.send(frame);
    })
    .unwrap();
    let (server, _) = listener.accept().unwrap();
    (client, server, seen)
}

fn line(frame: &ToClient) -> Vec<u8> {
    let mut line = serde_json::to_vec(frame).unwrap();
    line.push(b'\n');
    line
}

#[test]
fn a_command_goes_as_one_line_and_waits_for_its_line_back() {
    let (client, server, _seen) = connected("call");
    let answering = std::thread::Builder::new().name("server".into());
    let answering = answering
        .spawn(move || {
            let mut lines = BufReader::new(server.try_clone().unwrap()).lines();
            let frame: ToServer = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
            let ToServer::Call { id, command } = frame else {
                panic!("{frame:?}");
            };
            let done = ToClient::Done {
                id,
                done: Done::Work { running: vec![] },
            };
            (&server).write_all(&line(&done)).unwrap();
            command
        })
        .unwrap();
    assert_eq!(
        client.call(Command::Work, WAIT),
        Ok(Done::Work { running: vec![] })
    );
    assert_eq!(answering.join().unwrap(), Command::Work);
}

#[test]
fn what_else_the_server_says_reaches_the_interface_in_order_past_a_bad_line() {
    let (_client, mut server, seen) = connected("order");
    let note = |text: &str| ToClient::Event {
        session: None,
        event: Event::Note(text.into()),
        seq: None,
    };
    server.write_all(&line(&note("one"))).unwrap();
    server.write_all(b"not a frame\n").unwrap();
    server.write_all(&line(&note("two"))).unwrap();
    assert_eq!(seen.recv_timeout(WAIT).unwrap(), note("one"));
    assert_eq!(seen.recv_timeout(WAIT).unwrap(), note("two"));
}

#[test]
fn a_server_that_hangs_up_is_gone_and_a_client_dropped_hangs_up() {
    let (client, server, _seen) = connected("gone");
    drop(server);
    let deadline = std::time::Instant::now() + WAIT;
    while client.send(Command::Leave) != Err(CallError::Gone) {
        assert!(std::time::Instant::now() < deadline, "never gone");
        std::thread::yield_now();
    }
    assert_eq!(client.call(Command::List, WAIT), Err(CallError::Gone));

    let (client, mut server, _seen) = connected("drop");
    // Set while the client is there: macOS refuses it on a socket its peer has shut down.
    server.set_read_timeout(Some(WAIT)).unwrap();
    drop(client);
    assert_eq!(
        server.read(&mut [0; 8]).unwrap(),
        0,
        "the server sees the end"
    );
}
