//! What a command the agent runs can reach of llx itself: the provider's key is in llx's
//! environment, which another process of the same user can read from `/proc` unless llx has made
//! itself non-dumpable (a process's children set it back on exec, so their own are readable).

use serde_json::json;

use crate::common::home::{FAKE_KEY, Home, text};
use crate::common::model::{Model, Reply, said};

#[test]
fn a_command_cannot_read_the_key_from_llxs_environment() {
    // Root reads any process's files in /proc, dumpable or not: nothing to show as root.
    // SAFETY: geteuid takes nothing and only reads this process's credentials.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let home = Home::new();
    // The shell the command runs in is llx's child, so $PPID is llx.
    let command = "cat /proc/$PPID/environ 2>&1 | tr '\\0' '\\n'";
    let model = Model::start(vec![
        Reply::call("bash", json!({ "command": command })),
        Reply::done("Done."),
    ]);
    let out = home.exec(&model, &["exec", "-y", "look"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let told = said(&model, 1);
    assert!(told.contains("Permission denied"), "{told}");
    assert!(!told.contains(FAKE_KEY), "the key was read: {told}");
}
