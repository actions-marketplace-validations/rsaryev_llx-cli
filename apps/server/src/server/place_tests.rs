use serde_json::json;

use super::*;
use crate::compose::{Models, assemble};
use crate::interrupt::Interrupt;
use crate::testing::{Scratch, ScriptedChat, quiet, says};

/// A session opened from a client elsewhere: its commands run there, with what the client has
/// and this process lacks.
#[test]
fn a_session_works_in_the_clients_directory_with_the_clients_environment() {
    let (scratch, there) = (Scratch::new(), Scratch::new());
    there.file("marker.txt", "");
    let place = Place {
        cwd: there.path().display().to_string(),
        env: vec![
            ("PATH".into(), std::env::var("PATH").unwrap_or_default()),
            ("LLX_ONLY_THE_CLIENT_HAS".into(), "the client's".into()),
        ],
        ..Place::default()
    };
    assert!(std::env::var_os("LLX_ONLY_THE_CLIENT_HAS").is_none());
    let mut setup = crate::testing::setup(&scratch, crate::modes::built("bypass"), false);
    setup.workspace = workspace(&place).unwrap();
    let command = r#"ls; echo "[$LLX_ONLY_THE_CLIENT_HAS]""#;
    let chat = ScriptedChat::new(vec![
        crate::testing::call("b", "bash", json!({ "command": command })),
        says("done"),
    ]);
    let models = Models {
        chat: &chat,
        live: None,
        adviser: None,
        namer: None,
    };
    let (shared, seat) = setup.split();
    let root = std::fs::canonicalize(there.path()).unwrap();
    assemble(&shared, seat, models, &quiet(), &Interrupt::new(), |rig| {
        assert_eq!(rig.shared.workspace.root(), root);
        rig.task("what is there?");
        let shown = crate::testing::results(rig);
        let report = &shown[0].1;
        assert!(report.contains("marker.txt\n[the client's]"), "{report}");
        0
    });
}

#[test]
fn a_place_that_is_not_a_directory_is_refused() {
    let scratch = Scratch::new();
    scratch.file("a.txt", "");
    let cwd = scratch.path().join("a.txt").display().to_string();
    let place = Place {
        cwd,
        ..Place::default()
    };
    let why = workspace(&place).unwrap_err();
    assert!(why.ends_with("a.txt: not a directory"), "{why}");
}
