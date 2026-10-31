use std::sync::mpsc::{Receiver, channel};

use protocol::{Approval, Event};

use super::*;

const WAIT: Duration = Duration::from_secs(5);

/// A client and the server's ends of its channels, with what reaches the interface.
#[expect(
    clippy::disallowed_methods,
    reason = "the in-process transport's own channels, as the binary makes them"
)]
fn connected() -> (
    Client,
    Receiver<ToServer>,
    Sender<ToClient>,
    Receiver<ToClient>,
) {
    let (to_server, at_server) = channel();
    let (from_server, at_client) = channel();
    let (shown, seen) = channel();
    let client = Client::in_process(to_server, at_client, move |frame| {
        let _ = shown.send(frame);
    })
    .unwrap();
    (client, at_server, from_server, seen)
}

/// A server that does each command it is sent with `done`, on a thread of its own.
fn answering(at_server: Receiver<ToServer>, from_server: Sender<ToClient>, done: Done) {
    let server = std::thread::Builder::new().name("server".into());
    let _ = server.spawn(move || {
        for frame in at_server {
            if let ToServer::Call { id, .. } = frame {
                let _ = from_server.send(ToClient::Done {
                    id,
                    done: done.clone(),
                });
            }
        }
    });
}

#[test]
fn a_command_waits_for_what_is_done_with_it() {
    let (client, at_server, from_server, _seen) = connected();
    answering(at_server, from_server, Done::Work { running: vec![] });
    assert_eq!(
        client.call(Command::Work, WAIT),
        Ok(Done::Work { running: vec![] })
    );
}

#[test]
fn what_the_server_could_not_do_comes_back_as_its_reason() {
    let (client, at_server, from_server, _seen) = connected();
    let failed = Done::Failed {
        why: "8 sessions are open".into(),
    };
    answering(at_server, from_server, failed);
    let target = protocol::Target::New;
    assert_eq!(
        client.call(Command::Open { target }, WAIT),
        Err(CallError::Failed("8 sessions are open".into()))
    );
}

#[test]
fn everything_else_the_server_says_reaches_the_interface_in_order() {
    let (_client, _at_server, from_server, seen) = connected();
    for text in ["one", "two"] {
        let event = Event::Note(text.into());
        from_server
            .send(ToClient::Event {
                session: None,
                event,
                seq: None,
            })
            .unwrap();
    }
    from_server.send(ToClient::Resolved { id: 3 }).unwrap();
    let notes: Vec<ToClient> = seen.iter().take(3).collect();
    assert_eq!(notes[2], ToClient::Resolved { id: 3 });
    assert!(matches!(&notes[0], ToClient::Event { event: Event::Note(t), .. } if t == "one"));
    assert!(matches!(&notes[1], ToClient::Event { event: Event::Note(t), .. } if t == "two"));
}

#[test]
fn a_server_that_never_answers_times_out_and_one_that_is_gone_says_so() {
    let (client, at_server, from_server, _seen) = connected();
    assert_eq!(
        client.call(Command::List, Duration::from_millis(50)),
        Err(CallError::Timeout)
    );
    drop((at_server, from_server));
    assert_eq!(client.call(Command::List, WAIT), Err(CallError::Gone));
    assert_eq!(client.send(Command::Leave), Err(CallError::Gone));
}

#[test]
fn a_reply_reaches_the_server_under_its_question() {
    let (client, at_server, _from_server, _seen) = connected();
    client.reply(9, Reply::Approval(Approval::Yes)).unwrap();
    assert_eq!(
        at_server.recv().unwrap(),
        ToServer::Reply {
            id: 9,
            reply: Reply::Approval(Approval::Yes)
        }
    );
}
