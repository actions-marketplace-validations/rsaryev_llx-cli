//! What the server does with what an interface sends: each reply goes to the question it
//! answers, each command is done and said done (W1: every command gets one answer). A command
//! never waits on a session: what a lane must do is posted to it. Any number of connections
//! serve one peer, each its own client of it (W5, W6).

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};

use protocol::{Command, Done, ToClient, ToServer};

use super::host::Host;
use super::hub::ClientId;
use super::ui::{FIRST, Peer};
use crate::desk::Request;

/// What the server tells an interface of itself when it attaches.
pub(crate) struct Info {
    pub max_lanes: usize,
    /// What `/` offers, each command with what it does.
    pub commands: Vec<(String, String)>,
    /// The modes, each with what it does.
    pub modes: Vec<(String, String)>,
    /// The daemon this server is, when it is one.
    pub host: Option<Arc<Host>>,
}

/// Serve the peer's first client ([`Peer::new`]'s) until it is gone, from `first` when a frame
/// was already read from the connection (the daemon reads its handshake to know where the client
/// is): replies to their questions, lanes' commands to the desk, the person's entries to the
/// lanes' queues. Whether the person moved the session to the background: then its prompts wait
/// for the next interface. Otherwise, once the last interface is gone, they end with nothing, and
/// the desk, its requests' sender dropped with this connection, stops the lanes' work.
pub(crate) fn serve(
    first: Option<ToServer>,
    incoming: &Receiver<ToServer>,
    peer: &Arc<Peer>,
    desk: &Sender<Request>,
    info: &Info,
) -> bool {
    serving(first, incoming, peer, FIRST, desk, info)
}

/// A connection back to sessions moved to the background: attached at once, so it is given
/// what they did meanwhile (the ring, then the questions waiting), then served from `first`.
pub(crate) fn rejoin(
    first: ToServer,
    incoming: &Receiver<ToServer>,
    peer: &Arc<Peer>,
    out: Sender<ToClient>,
    desk: &Sender<Request>,
    info: &Info,
) -> bool {
    peer.inputs.back();
    let me = peer.hub.attach(out);
    serving(Some(first), incoming, peer, me, desk, info)
}

fn serving(
    first: Option<ToServer>,
    incoming: &Receiver<ToServer>,
    peer: &Peer,
    me: ClientId,
    desk: &Sender<Request>,
    info: &Info,
) -> bool {
    let mut kept = false;
    for frame in first.into_iter().chain(incoming) {
        match frame {
            ToServer::Reply { id, reply } => peer.replied(id, reply),
            ToServer::Call { id, command } => {
                // Kept only once the desk took it: a desk that ended keeps nothing.
                let background = command == Command::Background;
                let done = guarded(command, peer, desk, info);
                kept |= background && done == Done::Ok;
                peer.hub.to(me, ToClient::Done { id, done });
            }
        }
    }
    // The last interface is gone, and the work was not kept: nobody will answer what waits for
    // the person.
    let left = peer.hub.detach(me);
    if !kept && left == 0 {
        peer.inputs.leave();
        peer.abandon(None);
    }
    kept
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
        Command::Initialize { place, build, .. } => {
            // A daemon of another build may not speak the same frames: the client starts its own.
            if let Some(host) = info.host.as_ref().filter(|host| host.build != build) {
                let why = format!("the daemon is build {}, this llx {build}", host.build);
                return Done::Failed { why };
            }
            // The sessions this client opens work where it is.
            if let Some(place) = place {
                let _ = desk.send(Request::Attach(place));
            }
            Done::Initialized {
                version: protocol::VERSION,
                max_lanes: info.max_lanes,
                commands: info.commands.clone(),
                modes: info.modes.clone(),
                background: info.host.is_some(),
            }
        }
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
        Command::Background => to_desk(Request::Background),
        Command::Status => info
            .host
            .as_ref()
            .map_or_else(not_a_daemon, |host| host.status()),
        Command::Shutdown => info.host.as_ref().map_or_else(not_a_daemon, |host| {
            host.end();
            Done::Ok
        }),
    }
}

fn not_a_daemon() -> Done {
    Done::Failed {
        why: "not a daemon: this llx runs its sessions itself".into(),
    }
}

#[cfg(test)]
#[path = "serve_tests.rs"]
mod tests;
