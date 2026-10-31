use super::*;

#[test]
fn slash_commands_are_not_tasks() {
    let unknown = |text: &str| Some(Entry::Unknown(text.into()));
    for (typed, read) in [
        ("   ", None),
        (" /exit ", Some(Entry::Exit)),
        ("/quit", Some(Entry::Exit)),
        ("/help", Some(Entry::Help)),
        ("/new", Some(Entry::New)),
        ("/clear", Some(Entry::New)),
        ("/status", Some(Entry::Status)),
        (" /login ", Some(Entry::Login)),
        ("/login openai", unknown("login openai")),
        // Another spelling of /rewind.
        ("/undo", Some(Entry::Rewind)),
        ("/frobnicate", unknown("frobnicate")),
        (
            "add a test for /api/users",
            Some(Entry::Task("add a test for /api/users".into())),
        ),
        // Shift+Up and Shift+Down ask for more or less reasoning.
        (MORE_REASONING, Some(Entry::Reasoning(true))),
        (LESS_REASONING, Some(Entry::Reasoning(false))),
    ] {
        assert_eq!(entry(typed), read, "{typed:?}");
    }
}

#[test]
fn every_command_the_menu_offers_is_one_and_the_help_names_it() {
    for (name, _) in commands() {
        let read = entry(&format!("/{name}"));
        assert!(!matches!(read, None | Some(Entry::Unknown(_))), "/{name}");
        assert!(HELP.contains(&format!("  /{name} ")), "/{name} in the help");
    }
}

#[test]
fn mode_commands_switch_and_list() {
    assert_eq!(entry("/mode"), Some(Entry::Modes));
    assert_eq!(entry("/permissions"), Some(Entry::Modes));
    assert_eq!(entry("/mode plan"), Some(Entry::Switch("plan".into())));
    assert_eq!(
        entry("/permissions  review "),
        Some(Entry::Switch("review".into()))
    );
    assert_eq!(entry("\u{1b}[Z"), Some(Entry::CycleMode));
    assert_eq!(entry("\u{1b}[Z\n"), Some(Entry::CycleMode));
    assert_eq!(entry("/plan"), Some(Entry::Unknown("plan".into())));
    assert_eq!(entry("/exit now"), Some(Entry::Unknown("exit now".into())));
}
