use super::*;
use crate::testing::{answers, composed, fake_ui, users};

#[test]
fn switching_to_a_mode_that_is_not_one_changes_nothing() {
    composed(crate::modes::built("default"), true, answers(0), |rig| {
        assert_eq!(
            command(rig, &fake_ui().0, Entry::Switch("nope".into())),
            None
        );
        assert_eq!(rig.mode.get().name(), "default");
        assert_eq!(
            command(rig, &fake_ui().0, Entry::Switch("plan".into())),
            None
        );
        assert_eq!(rig.mode.get().name(), "plan");
        0
    });
}

#[test]
fn shift_tab_cycles_every_mode_and_tells_the_next_task() {
    composed(crate::modes::built("default"), true, answers(6), |rig| {
        for expected in [
            crate::modes::built("plan"),
            crate::modes::built("bypass"),
            crate::modes::built("default"),
        ]
        .into_iter()
        .cycle()
        .take(6)
        {
            assert_eq!(command(rig, &fake_ui().0, entry("\u{1b}[Z").unwrap()), None);
            assert_eq!(rig.mode.get(), expected);
            assert_eq!(rig.task("continue"), exit::OK);
            let messages = users(rig);
            assert!(
                messages[messages.len() - 2].starts_with(&format!(
                    "<mode_change>\nThe mode is now {}: ",
                    expected.name()
                )),
                "the next task must hear the cycled mode: {messages:?}"
            );
            assert_eq!(messages.last().unwrap(), "Goal: continue");
        }
        0
    });
}
