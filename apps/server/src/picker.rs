//! Choosing at the terminal: a title and options in, the index chosen out, or nothing when the
//! person cancels ([`Picker`]); and a line of text, or a secret hidden ([`Lines`]). A session's
//! `Ui` implements both (`server/ui.rs`), its interface asked through the protocol.

use protocol::{Adjustment, Group};

/// One option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    pub label: String,
    /// A short note after the label.
    pub detail: String,
    /// The option in effect now (or the default): Enter alone picks it.
    pub marked: bool,
}

impl Item {
    pub(crate) fn new(label: impl Into<String>, detail: impl Into<String>, marked: bool) -> Self {
        Self {
            label: label.into(),
            detail: detail.into(),
            marked,
        }
    }
}

pub(crate) trait Picker {
    /// The index of the option chosen; `None` when the person cancels.
    fn pick(&mut self, title: &str, items: &[Item]) -> Option<usize>;

    /// The row and its selected value, the rows in `groups`. The default, for test doubles,
    /// shows one list and keeps the initial value.
    fn pick_adjustable(
        &mut self,
        title: &str,
        items: &[Item],
        adjustments: &[Adjustment],
        _groups: &[Group],
    ) -> Option<(usize, usize)> {
        let at = self.pick(title, items)?;
        Some((
            at,
            adjustments.get(at).map_or(0, |adjustment| adjustment.at),
        ))
    }
}

pub(crate) trait Lines {
    /// A secret typed after `prompt`, not shown on the screen; `None` at the end of input.
    fn secret(&mut self, prompt: &str) -> Option<String>;

    /// A line typed after `prompt`, shown, closed unanswered (`None`) once `done` says so: a
    /// sign-in's callback came back while the person could still paste. The default, for test
    /// doubles, reads it as [`Lines::secret`] and never closes by itself.
    fn line_until(&mut self, prompt: &str, _done: &dyn Fn() -> bool) -> Option<String> {
        self.secret(prompt)
    }
}

/// What a sign-in flow needs at the terminal.
pub(crate) trait Terminal: Picker + Lines {}

impl<T: Picker + Lines> Terminal for T {}
