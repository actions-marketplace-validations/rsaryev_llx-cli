use std::sync::mpsc::channel;

use super::*;

/// Sessions moved to the background in `cwd`, as a connection's end hands them to the host.
#[expect(
    clippy::disallowed_methods,
    reason = "the peer's and the desk's channels, as the binary makes them"
)]
fn kept(cwd: &str) -> Kept {
    let (out, _gone) = channel();
    let (desk, _requests) = channel();
    Kept {
        cwd: cwd.into(),
        peer: Peer::new(out),
        desk,
    }
}

fn kept_count(host: &Host) -> usize {
    let Done::Status(status) = host.status() else {
        panic!("no status");
    };
    status.kept
}

#[test]
fn sessions_whose_desk_already_ended_are_never_kept() {
    let host = Host::new("b".into());
    let gone = kept("/a");
    host.forget(&gone.peer);
    host.keep(gone);
    assert_eq!(kept_count(&host), 0);
    assert!(host.take("/a").is_none());
    // Others are kept until taken.
    host.keep(kept("/b"));
    assert_eq!(kept_count(&host), 1);
    assert!(host.take("/b").is_some());
}
