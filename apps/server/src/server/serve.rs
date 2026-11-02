//! What the server does with what the interface sends: each reply goes to the question it
//! answers, each command is done and said done (W1: every command gets one answer). A command
//! never waits on a session: what a lane must do is posted to it.

use std::sync::mpsc::{Receiver, Sender};

use protocol::{Command, Done, ToClient, ToServer};

use super::ui::Peer;
use crate::desk::Request;

/// What the server tells the interface of itself at the handshake.
pub(crate) struct Info {
    pub max_lanes: usize,
    /// What `/` offers, each command with what it does.
    pub commands: Vec<(String, String)>,
    /// The modes, each with what it does.
    pub modes: Vec<(String, String)>,
}

/// Serve the interface until it is gone: replies to their questions, lanes' commands to the
/// desk, the person's entries to the lanes' queues. Once it is gone the person's prompts end with
/// nothing, and the desk, its requests' sender dropped with it, stops the lanes' work.
pub(crate) fn serve(
    incoming: &Receiver<ToServer>,
    peer: &Peer,
    desk: &Sender<Request>,
    info: &Info,
) {
    for frame in incoming {
        match frame {
            ToServer::Reply { id, reply } => peer.replied(id, reply),
            ToServer::Call { id, command } => {
                let done = guarded(command, peer, desk, info);
                peer.send(ToClient::Done { id, done });
            }
        }
    }
    peer.inputs.leave();
    peer.abandon(None);
}

/// [`done`], a panic in it that command's failure, never the connection's end (W8). The lints
/// keep panics out of working code; this is for the one that gets through (an index, a library).
fn guarded(command: Command, peer: &Peer, desk: &Sender<Request>, info: &Info) -> Done {
    let doing = std::panic::AssertUnwindSafe(|| done(command, peer, desk, info));
    std::panic::catch_unwind(doing).unwrap_or_else(|panic| {
        let what = (panic.downcast_ref::<&str>().map(|why| why.to_string()))
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        Done::Failed {
            why: format!("the server failed doing it: {what}"),
        }
    })
}

/// What a command for a session that is not open comes to.
fn not_open(session: &str) -> Done {
    Done::Failed {
        why: format!("no session {session} is open"),
    }
}

fn done(command: Command, peer: &Peer, desk: &Sender<Request>, info: &Info) -> Done {
    let to_desk = |request| match desk.send(request) {
        Ok(()) => Done::Ok,
        Err(_gone) => Done::Failed {
            why: "the sessions have ended".into(),
        },
    };
    match command {
        Command::Initialize => Done::Initialized {
            max_lanes: info.max_lanes,
            commands: info.commands.clone(),
            modes: info.modes.clone(),
        },
        // A lane that ended keeps nothing: what comes for it is refused, never kept. One that
        // is ending may still be listed as working; its queue, forgotten first, refuses it.
        Command::Input(input) => {
            let session = input.session.clone();
            let busy = peer.work.busy(&session);
            match busy.is_some_and(|busy| peer.inputs.input(input, busy)) {
                true => Done::Ok,
                false => not_open(&session),
            }
        }
        Command::Interrupt { session } => Done::Interrupted {
            again: crate::interrupt::stop(&session),
        },
        Command::Halt { id } => {
            crate::interrupt::stop(&id);
            Done::Ok
        }
        Command::StopJob { lane, job } => {
            crate::interrupt::stop(&crate::interrupt::job_key(&lane, &job));
            Done::Ok
        }
        Command::Close { session } if peer.work.busy(&session).is_none() => not_open(&session),
        Command::Close { session } => {
            peer.inputs.close(&session);
            peer.abandon(Some(&session));
            Done::Ok
        }
        Command::Open { target } => to_desk(Request::Open(target)),
        Command::List => to_desk(Request::List),
        Command::QuitAll => to_desk(Request::Quit),
        Command::Leave => {
            peer.inputs.leave();
            peer.abandon(None);
            Done::Ok
        }
        Command::Work => Done::Work {
            running: peer.work.running(),
        },
    }
}

#[cfg(test)]
#[path = "serve_tests.rs"]
mod tests;
