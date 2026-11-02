//! The binary's side of a run (ARCHITECTURE.md D7), the one place that knows both the server and
//! the interface: the interface at this terminal, attached to the server in this process, and
//! each lane's trace as the interface draws it. The server takes it as its `Front`.

#[cfg(test)]
mod test_support;

use std::any::Any;
use std::io::IsTerminal;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};

use client::Client;
use protocol::{Offer, ToClient, ToServer};

use llx::commands::run::{Front, Unready};
use llx::interrupt;
use llx::output::{Answer, Drawing, Drawn};

/// This process at its terminal.
pub(crate) struct Here;

impl Front for Here {
    fn attach(
        &self,
        root: &Path,
        to_server: Sender<ToServer>,
        from_server: Receiver<ToClient>,
        offer: Offer,
    ) -> Result<Box<dyn Any>, Unready> {
        // A signal that quits at the prompt ends the process on the quitting thread: the
        // terminal is given back there.
        interrupt::on_quit(tui::restore);
        let connect: tui::Connect =
            Box::new(move |deliver| Client::in_process(to_server, from_server, deliver));
        let running = tui::start(tui::Start {
            connect,
            offer,
            root,
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
