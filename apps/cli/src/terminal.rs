//! The binary's side of a run (ARCHITECTURE.md D7): what stays in `apps/cli`, the server and the
//! interface being crates of their own. It is the one place that knows both: the
//! interface at this terminal, attached to the daemon (`remote`) or to a server in this process,
//! each lane's trace as the interface draws it, and the daemon's client side (`daemon`: its
//! status, reach, start and stop). The server takes it as its `Front`.

mod daemon;
mod remote;
#[cfg(test)]
mod test_support;

use std::any::Any;
use std::io::IsTerminal;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use client::Client;
use protocol::{ToClient, ToServer};

use llx::commands::run::{Front, Unready};
use llx::interrupt;
use llx::output::{Answer, Drawing, Drawn};

/// This process at its terminal.
pub(crate) struct Here;

impl Front for Here {
    fn remote(&self, root: &Path) -> Result<(), Unready> {
        remote::run(root).map_err(Unready)
    }

    fn attach(
        &self,
        root: &Path,
        to_server: Sender<ToServer>,
        from_server: Receiver<ToClient>,
    ) -> Result<Box<dyn Any>, Unready> {
        let connect: tui::Connect =
            Box::new(move |deliver| Client::in_process(to_server, from_server, deliver));
        let running = tui::start(tui::Start {
            connect,
            root,
            build: String::new(),
            quit_now: interrupt::quit_now,
        })
        .map_err(|error| Unready(error.to_string()))?;
        Ok(Box::new(running))
    }

    fn quit(&self) {
        tui::restore();
        interrupt::quit_now();
    }

    fn drawing(&self) -> Drawing {
        drawn
    }

    fn status(&self, path: &Path, wait: Duration) -> Option<protocol::Status> {
        daemon::status(path, wait)
    }

    fn stop(&self, path: &Path) -> bool {
        daemon::stop(path)
    }
}

/// A lane's trace on stderr, drawn by the interface's code from the events the server tells it,
/// for the stderr and stdout the process has now.
fn drawn() -> Drawn {
    let mut trace = tui::Trace::here();
    let answer = (std::io::stdout().is_terminal()).then(|| Box::new(trace.answer()) as Answer);
    Drawn {
        trace: Box::new(move |event| trace.draw(event)),
        answer,
    }
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
