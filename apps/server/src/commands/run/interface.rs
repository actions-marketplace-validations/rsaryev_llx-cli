//! A session's interface and its server: in this process the binary's interface over channels to
//! a server on a thread; the daemon serves each socket's client with the same server
//! (`serving`).

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use protocol::{Place, ToClient, ToServer};
use tools::workspace::Workspace;

use super::{Attached, Front, NO_INTERFACE, RUN_EXEC, Unready};
use crate::commands::entry;
use crate::desk;
use crate::error::Failure;
use crate::modes::Modes;
use crate::server::host::Host;
use crate::server::{Peer, Ui, serve};

/// A session takes the terminal for its interface (the binary's, `front`), its server in this
/// process; one that cannot draw there (no size to draw at) is a usage error, as having no
/// terminal is. The place the interface attached from comes with it.
pub(super) fn in_process(
    root: &Workspace,
    front: &'static dyn Front,
) -> Result<(Attached, Place), Failure> {
    let why = |error: String| Failure::Usage(format!("{NO_INTERFACE} ({error}): {RUN_EXEC}"));
    #[expect(
        clippy::disallowed_methods,
        reason = "the in-process transport: the server never waits on the interface (W6), the \
                  interface never on the desk; one frame per thing done or key pressed"
    )]
    let ((to_server, incoming), (out, from_server)) = (channel(), channel());
    // In process the desk ends with the interface: its requests' end stops the work.
    let ended = Box::new(|_, _, _| {});
    let served = serving(None, incoming, out, None, ended);
    let (peer, requests) = served.map_err(|error| why(error.to_string()))?;
    let interface = front.attach(root.root(), to_server, from_server);
    let interface = interface.map_err(|Unready(error)| why(error))?;
    // Said before the interface was told it is attached: the first of the desk's requests.
    let Ok(desk::Request::Attach(place)) = requests.try_recv() else {
        return Err(Failure::Usage(format!("{NO_INTERFACE}: it gave no place")));
    };
    // A newer model catalog, for the next session: nothing here waits on it.
    crate::providers::refresh_catalog();
    let attached = Attached {
        held: Some(interface),
        ui: Ui::new(peer),
        requests,
    };
    Ok((attached, place))
}

/// What becomes of a desk once its interface's connection ends: whether the person kept the work
/// (moved it to the background), its peer, and its requests' sender.
pub(crate) type Left = Box<dyn FnOnce(bool, Arc<Peer>, Sender<desk::Request>) + Send>;

/// A server for one interface, on a thread of its own, from `first` (a frame already read):
/// what it serves the sessions, and what the interface asks of the desk; `ended` once the
/// connection is gone. `host`: the daemon it is part of, when it is one.
pub(crate) fn serving(
    first: Option<ToServer>,
    incoming: Receiver<ToServer>,
    out: Sender<ToClient>,
    host: Option<Arc<Host>>,
    ended: Left,
) -> std::io::Result<(Arc<Peer>, Receiver<desk::Request>)> {
    #[expect(
        clippy::disallowed_methods,
        reason = "the desk's requests: one per thing the person asks of it, never waited on"
    )]
    let (desk, requests) = channel();
    let peer = Peer::new(out);
    let served = Arc::clone(&peer);
    let info = info(host);
    let server = std::thread::Builder::new().name("llx-serve".into());
    server.spawn(move || {
        let kept = serve::serve(first, &incoming, &served, &desk, &info);
        ended(kept, served, desk);
    })?;
    Ok((peer, requests))
}

/// What a server says of itself: what it offers, and what it can do (a daemon keeps work).
pub(crate) fn info(host: Option<Arc<Host>>) -> serve::Info {
    serve::Info {
        max_lanes: desk::MAX_LANES,
        commands: entry::commands(),
        // A server's settings that cannot be read refused its run already; here the list is all.
        modes: (Modes::load()
            .unwrap_or_else(|_| Modes::built_in())
            .all()
            .iter())
        .map(|mode| (mode.name().to_string(), mode.hint().to_string()))
        .collect(),
        host,
    }
}
