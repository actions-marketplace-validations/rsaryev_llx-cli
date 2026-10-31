//! Test-only: a person at a real interface in this process (`tui::testing::Person`), attached
//! to a desk the server serves through the protocol's frames, acting as the interface's thread
//! does. The binary's, the one crate that knows both the interface and the server
//! (ARCHITECTURE.md L4, D7).

use llx::test_support::{Gated, Link, MAX_LANES, Scratch, Server, served};
use tui::testing::Person;

/// A session at the desk, its first task `task`, its lanes thinking with `chats` in the order
/// they open (a lane past them panics as it starts); `person` acts, then leaves, and the desk
/// must end. The session store, for what was recorded.
pub(crate) fn at_desk(
    chats: &[&Gated],
    task: Option<&str>,
    person: impl FnOnce(&mut Person),
) -> Scratch {
    at_desk_with(chats, task, &|| panic!("the desk quit hard"), person)
}

/// [`at_desk`], with `quit` for what ends the process when lanes do not stop in time.
pub(crate) fn at_desk_with(
    chats: &[&Gated],
    task: Option<&str>,
    quit: &(dyn Fn() + Sync),
    person: impl FnOnce(&mut Person),
) -> Scratch {
    connected(chats, task, quit, |mut at, _| {
        person(&mut at);
        at.leave();
        at.until_over();
    })
}

/// A session at the desk as [`at_desk_with`] makes it, `person` given the connection to end it
/// as they choose, and the server's side of it; the desk must end within 10 s after.
pub(crate) fn connected(
    chats: &[&Gated],
    task: Option<&str>,
    quit: &(dyn Fn() + Sync),
    person: impl FnOnce(Person, &Server),
) -> Scratch {
    served(chats, task, quit, |Link { frames, to_server }, peer| {
        person(Person::new(frames, to_server, MAX_LANES), peer);
    })
}
