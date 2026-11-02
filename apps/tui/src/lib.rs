//! The terminal interface: what a person sees and types. Finished blocks go into the terminal's
//! own scrollback once (so scrolling, selecting and copying stay the terminal's); only a small
//! live area at the bottom is redrawn. Every block is a pure function from state and width to
//! styled lines, which the snapshot tests pin.
//!
//! One thread owns the terminal: it reads the keys (from a reader thread), takes the server's
//! frames through a [`Client`], and draws. The sessions run in the server; the interface knows
//! them only by what the protocol carries (ARCHITECTURE.md L7).

mod agents;
mod app;
mod attach;
mod cells;
mod clipboard;
mod composer;
mod drawing;
mod event;
mod farewell;
mod here;
mod keys;
mod list;
mod live;
mod markdown;
mod mention;
mod opening;
mod printable;
mod questions;
mod queue;
mod quitting;
mod route;
mod screen;
mod search;
mod select;
mod sessions;
mod spend;
mod steer;
mod style;
mod terminal;
#[cfg(test)]
mod test_support;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
mod time;
mod tool_cell;
mod trace;
mod tree;
mod views;
mod within;
mod wrap;

use std::io::{self, Write};
use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;

use client::Client;
use protocol::{Command, Done, Event, Offer, ToClient};

use self::app::App;
use self::event::Input;
pub use self::terminal::restore;
pub use self::trace::Trace;

/// The interface running; dropping it gives the terminal back.
pub struct Running {
    /// Where the interface's thread takes what comes: told here the session is over.
    inbox: Sender<Input>,
    thread: Option<JoinHandle<Vec<ratatui::text::Line<'static>>>>,
    /// What the interface said it will print once the terminal is given back.
    farewell: Vec<ratatui::text::Line<'static>>,
}

/// Where the server's frames that are not answers go, until the server is gone.
pub type Deliver = Box<dyn Fn(ToClient) + Send>;

/// The client to the server, made with where its frames go: over channels to the server in this
/// process.
pub type Connect<'a> = Box<dyn FnOnce(Deliver) -> io::Result<Client> + 'a>;

/// What the interface is given: its connection to the server, what the server offers (W3), the
/// directory for `@` (where the sessions work), and how to end the process at once.
pub struct Start<'a> {
    pub connect: Connect<'a>,
    pub offer: Offer,
    pub root: &'a std::path::Path,
    pub quit_now: fn() -> !,
}

/// Takes the terminal, connects to the server and starts drawing.
pub fn start(start: Start) -> io::Result<Running> {
    let (width, height) = crossterm::terminal::size()?;
    terminal::on_signals(start.quit_now);
    crossterm::terminal::enable_raw_mode()?;
    let mut out = io::stdout();
    // Nowhere to draw (stdout's reader gone): the terminal goes back as it was.
    if let Err(error) = out
        .write_all(terminal::ENTER.as_bytes())
        .and_then(|()| out.flush())
    {
        restore();
        return Err(error);
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));
    match attach(start, width, height) {
        Ok(running) => Ok(running),
        Err(error) => {
            restore();
            Err(error)
        }
    }
}

/// The key reader, the client and the interface's thread, once the terminal is the interface's.
fn attach(start: Start, width: u16, height: u16) -> io::Result<Running> {
    #[expect(
        clippy::disallowed_methods,
        reason = "the interface must never make the server or the keys wait: it takes \
                  everything that came before each frame it draws"
    )]
    let (inbox, taken) = channel();
    let keys = inbox.clone();
    let reader = std::thread::Builder::new().name("llx-keys".into());
    reader.spawn(move || terminal::read_keys(&keys))?;
    let gone = Gone(inbox.clone());
    let client = (start.connect)(Box::new(move |frame| {
        let _ = gone.0.send(Input::Frame(frame));
    }))?;
    let Offer {
        max_lanes,
        commands,
        modes,
    } = start.offer;
    let mut app = App::new(usize::from(width), usize::from(height), commands);
    let files = inbox.clone();
    let arrive = move |list| {
        let _ = files.send(Input::Files(list));
    };
    app.files = mention::Files::new(start.root.to_path_buf(), Box::new(arrive));
    let asking = client.clone();
    // Asked only when the quit question needs it.
    app.work = Box::new(move || match asking.call(Command::Work, drawing::ANSWER) {
        Ok(Done::Work { running, .. }) => running,
        _ => vec![],
    });
    app.max_lanes = max_lanes;
    app.modes = modes;
    let seat = drawing::Seat {
        client,
        quit_now: start.quit_now,
    };
    let drawing = std::thread::Builder::new().name("llx-interface".into());
    let thread = drawing.spawn(move || drawing::draw(app, &taken, &seat))?;
    Ok(Running {
        inbox,
        thread: Some(thread),
        farewell: vec![],
    })
}

/// Where the server's frames went: when the server is gone (its thread ended), the interface is
/// told so and ends.
struct Gone(Sender<Input>);

impl Drop for Gone {
    fn drop(&mut self) {
        for event in [Event::Error("llx: the server is gone".into()), Event::Quit] {
            let _ = self.0.send(server_event(event));
        }
    }
}

/// What the server says of itself, as the interface's thread takes it.
fn server_event(event: Event) -> Input {
    Input::Frame(ToClient::Event {
        session: None,
        event,
    })
}

impl Running {
    /// Until the interface ends by itself: the server said the session is over, or is gone.
    pub fn wait(mut self) {
        self.join();
    }

    fn join(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.farewell = thread.join().unwrap_or_default();
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = (self.inbox).send(server_event(Event::Quit));
        self.join();
        restore();
        terminal::say(&self.farewell);
    }
}
