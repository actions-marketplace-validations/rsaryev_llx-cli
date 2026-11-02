//! A session's interface and its server: the binary's interface over channels to a server on a
//! thread of this process.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use protocol::{ToClient, ToServer};
use tools::workspace::Workspace;

use super::{Attached, Front, NO_INTERFACE, RUN_EXEC, Unready};
use crate::commands::entry;
use crate::desk;
use crate::error::Failure;
use crate::modes::Modes;
use crate::server::{Peer, Ui, serve};

/// A session takes the terminal for its interface (the binary's, `front`), its server in this
/// process; one that cannot draw there (no size to draw at) is a usage error, as having no
/// terminal is.
pub(super) fn in_process(root: &Workspace, front: &'static dyn Front) -> Result<Attached, Failure> {
    let why = |error: String| Failure::Usage(format!("{NO_INTERFACE} ({error}): {RUN_EXEC}"));
    #[expect(
        clippy::disallowed_methods,
        reason = "the seam's channels: the server never waits on the interface (W6), the \
                  interface never on the desk; one frame per thing done or key pressed"
    )]
    let ((to_server, incoming), (out, from_server)) = (channel(), channel());
    let (peer, requests) = serving(incoming, out).map_err(|error| why(error.to_string()))?;
    let interface = front.attach(root.root(), to_server, from_server);
    let held = interface.map_err(|Unready(error)| why(error))?;
    // A newer model catalog, for the next session: nothing here waits on it.
    crate::providers::refresh_catalog();
    Ok(Attached {
        held,
        ui: Ui::new(peer),
        requests,
    })
}

/// A server for the interface, on a thread of its own: what it serves the sessions, and what the
/// interface asks of the desk. The desk's requests end with the interface, which stops the work.
pub(crate) fn serving(
    incoming: Receiver<ToServer>,
    out: Sender<ToClient>,
) -> std::io::Result<(Arc<Peer>, Receiver<desk::Request>)> {
    #[expect(
        clippy::disallowed_methods,
        reason = "the desk's requests: one per thing the person asks of it, never waited on"
    )]
    let (desk, requests) = channel();
    let peer = Peer::new(out);
    let served = Arc::clone(&peer);
    let info = info();
    let server = std::thread::Builder::new().name("llx-serve".into());
    server.spawn(move || serve::serve(&incoming, &served, &desk, &info))?;
    Ok((peer, requests))
}

/// What the server says of itself: what it offers.
pub(crate) fn info() -> serve::Info {
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
    }
}
