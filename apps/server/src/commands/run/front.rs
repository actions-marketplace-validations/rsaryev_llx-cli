//! What the server's run takes from the binary that runs it (ARCHITECTURE.md D7): the terminal a
//! session is shown at, in this process, and how each lane's trace is drawn. The server names no
//! interface and no client; the binary implements this (its `terminal`, in apps/cli) and hands it
//! in with the command line.

use std::any::Any;
use std::fmt;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};

use protocol::{ToClient, ToServer};

use crate::output::Drawing;

/// The binary's side of a run. One value for the process, passed down to every session, so each
/// lane's trace is drawn as the binary draws it.
pub trait Front: Send + Sync {
    /// The interface at this terminal, over `to_server` and `from_server` to its server in this
    /// process: what holds the terminal, given back when it is dropped.
    fn attach(
        &self,
        root: &Path,
        to_server: Sender<ToServer>,
        from_server: Receiver<ToClient>,
    ) -> Result<Box<dyn Any>, Unready>;
    /// The terminal given back, and the process ended at once: lanes that do not stop in time.
    fn quit(&self);
    /// How each lane's trace is drawn.
    fn drawing(&self) -> Drawing;
}

/// Why the terminal could not be had for a session, as the person is told it: the session then
/// does not run.
#[derive(Debug)]
pub struct Unready(pub String);

impl fmt::Display for Unready {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
