//! The terminal's keys: read as they come, by a thread that sleeps until they do, and handed to
//! the person's editor while it runs.

use super::LONG;
use crate::common::model::Model;
use crate::common::term::{Term, key};

/// The key reader waits on the terminal, not on a clock (ARCHITECTURE.md R9): while nobody types,
/// its thread does not wake. Its wakes are counted by the system (Linux's `/proc`).
#[cfg(target_os = "linux")]
#[test]
fn the_key_reader_sleeps_while_nobody_types() {
    let model = Model::start(vec![]);
    let term = Term::start(&model, &[]);
    let tasks = std::path::PathBuf::from(format!("/proc/{}/task", term.pid()));
    let named = |task: &std::path::PathBuf| {
        std::fs::read_to_string(task.join("comm")).is_ok_and(|name| name.trim() == "llx-keys")
    };
    let reader = (std::fs::read_dir(tasks).unwrap().flatten())
        .map(|task| task.path())
        .find(named)
        .expect("the key reader's thread");
    let woken = || {
        let status = std::fs::read_to_string(reader.join("status")).unwrap();
        let line = status
            .lines()
            .find_map(|l| l.strip_prefix("voluntary_ctxt_switches:"));
        line.unwrap().trim().parse::<u64>().unwrap()
    };
    let before = woken();
    #[expect(
        clippy::disallowed_methods,
        reason = "the idle time whose wakes are counted"
    )]
    std::thread::sleep(std::time::Duration::from_secs(1));
    let woke = woken() - before;
    assert!(
        woke <= 2,
        "the key reader woke {woke} times in an idle second"
    );
}

/// Ctrl+G hands the terminal to `$EDITOR`: what is typed while it runs is the editor's, and what
/// it saved comes back to the composer.
#[test]
fn the_editor_has_the_keys_while_it_runs() {
    let model = Model::start(vec![]);
    let place = std::env::temp_dir().join(format!("llx-editor-{}", std::process::id()));
    std::fs::create_dir_all(&place).unwrap();
    let editor = place.join("editor.sh");
    let script =
        "#!/bin/sh\nprintf 'EDITING\\n'\nIFS= read -r line\nprintf '%s' \"$line\" > \"$1\"\n";
    std::fs::write(&editor, script).unwrap();
    let mode = <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755);
    std::fs::set_permissions(&editor, mode).unwrap();
    let mut term = Term::spawn(&model, &[], &[("VISUAL", editor.to_str().unwrap())]);
    term.wait_for("❯", LONG);
    term.press("\x07");
    term.wait_for("EDITING", LONG);
    term.type_text("from the editor").press(key::ENTER);
    term.wait_for("❯ from the editor", LONG);
    let _ = std::fs::remove_dir_all(place);
}
