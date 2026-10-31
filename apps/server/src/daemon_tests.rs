use std::time::Duration;

use super::*;
use crate::testing::Scratch;

#[test]
fn each_config_has_its_own_socket_in_the_persons_own_directory() {
    let home = socket_path(Path::new("/home/a/.config/llx"));
    let again = socket_path(Path::new("/home/a/.config/llx"));
    let test_home = socket_path(Path::new("/tmp/test-home/.config/llx"));
    assert_eq!(home, again);
    assert_ne!(home, test_home);
    // SAFETY: getuid has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    assert_eq!(
        home.parent().unwrap(),
        runtime_dir().join(format!("llx-{uid}"))
    );
    let name = home.file_name().unwrap().to_str().unwrap();
    assert!(name.ends_with(".sock") && name.len() <= 24, "{name}");
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "a backoff while a child of another test holds the lock between fork and exec"
)]
fn a_second_daemon_is_refused_the_lock_until_the_first_lets_go() {
    let scratch = Scratch::new();
    let config = scratch.path().join("config");
    let first = lock(&config).unwrap();
    assert!(first.is_some());
    assert!(
        lock(&config).unwrap().is_none(),
        "two daemons for one config"
    );
    drop(first);
    // A command another test runs at the same time starts by fork, then exec (`pre_exec` rules out
    // posix_spawn): between the two the child holds every descriptor of this process, the lock's
    // among them, so the lock can stay taken for an instant after `first` lets go.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let again = loop {
        match lock(&config).unwrap() {
            Some(held) => break Some(held),
            None if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            None => break None,
        }
    };
    assert!(
        again.is_some(),
        "the lock stayed taken after its holder let go"
    );
}

#[test]
fn a_client_finds_the_daemon_by_its_lock_whatever_its_own_runtime_directory() {
    let scratch = Scratch::new();
    let config = scratch.path().join("config");
    // Nobody holds it: where this process would put it.
    assert_eq!(socket_path(&config), own_socket(&config));
    let held = lock(&config).unwrap().unwrap();
    assert_eq!(socket_path(&config), own_socket(&config));
    // A daemon started where `$TMPDIR` was another says so in its lock: a client of a host
    // that sets another `$TMPDIR` goes to it, not to a second daemon the lock refuses.
    std::fs::write(
        config.join("daemon.lock"),
        "4242\n/elsewhere/llx-501/a.sock",
    )
    .unwrap();
    assert_eq!(
        socket_path(&config),
        PathBuf::from("/elsewhere/llx-501/a.sock")
    );
    drop(held);
    // Let go: the lock names nothing.
    assert_eq!(socket_path(&config), own_socket(&config));
}
