//! Quitting at the empty prompt: Ctrl-D, or Ctrl-C twice, leaves no prompt row behind.

use std::time::Duration;

use crate::common::eventually;
use crate::common::model::Model;
use crate::common::term::Term;

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "a test gives a late row the time to be drawn: there is no event to wait on"
)]
fn quitting_leaves_no_prompt_row_on_the_screen() {
    for keys in [&["\x04"][..], &["\x03", "\x03"]] {
        let model = Model::start(vec![]);
        let mut term = Term::start(&model, &[]);
        for key in keys {
            term.press(key);
        }
        let gone = || (!term.screen().contains('❯')).then_some(());
        let _ = eventually(Duration::from_secs(5), Duration::from_millis(50), gone);
        std::thread::sleep(Duration::from_millis(300));
        let screen = term.screen();
        assert!(!screen.contains('❯'), "{keys:?} left:\n{screen}");
    }
}
