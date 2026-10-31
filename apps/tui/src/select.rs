//! A choice put to the person: a title, numbered options, and an option that opens a line to type
//! on. Up and Down (or k and j) move, a number picks at once, Enter picks the marked one, and Esc
//! cancels (or, while typing, goes back to the options). No letter picks: words typed ahead as a
//! choice opens must not answer it. A list longer than its window is searchable instead: letters
//! go to a query that filters it (`search`), PageUp, PageDown, Home and End move too, and no
//! number picks. A list in groups shows one at a time, as tabs (`tabs`): Tab and Shift+Tab
//! switch, and a search looks in every one.
//!
//! ```text
//! Run this command?
//!   cargo test --workspace
//!
//! ❯ 1. Yes
//!   2. Yes, and don't ask again for bash
//!   3. No, and tell llx what to do differently
//!
//!   ↑↓ to choose · enter to confirm · esc to say no
//! ```

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::text::Line;

use super::composer::Composer;
use super::list::moved;
use super::search::{self, Search, Searched};
use tabs::Tabs;

mod preview;
mod render;
mod tabs;

/// Options shown at once; a longer list scrolls with the marked one.
const WINDOW: usize = 12;

#[derive(Default)]
pub(crate) struct Choice {
    pub label: String,
    /// Said after the label, dim.
    pub detail: String,
    /// Picking it opens a line to type on.
    pub types: bool,
    /// What it looks like, shown beside the options while it is marked; empty for nothing.
    pub preview: String,
}

impl Choice {
    pub(crate) fn new(label: &str) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }

    pub(crate) fn preview(mut self, preview: &str) -> Self {
        self.preview = preview.into();
        self
    }

    pub(crate) fn detail(mut self, detail: &str) -> Self {
        self.detail = detail.into();
        self
    }

    pub(crate) fn types(mut self) -> Self {
        self.types = true;
        self
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum Picked {
    Option(usize),
    Adjusted(usize, usize),
    /// The options checked, in order, where several may be.
    Many(Vec<usize>),
    /// An option that types, with what was typed.
    Typed(usize, String),
    Cancelled,
}

#[derive(Default)]
pub(crate) struct Select {
    title: Vec<Line<'static>>,
    /// The title line that names the session asking, while it is not the one shown.
    label: Option<usize>,
    choices: Vec<Choice>,
    at: usize,
    typing: Option<Composer>,
    /// What Esc does (and Ctrl-C, where it does more), as the hint says it: `esc cancel`,
    /// `esc no · ctrl-c stop`.
    escape: String,
    /// What is typed is a secret: shown as dots.
    hidden: bool,
    /// Only a line to type, with no options shown.
    direct: bool,
    /// Several options may be checked (Space), then sent together (Enter).
    multiple: bool,
    checked: Vec<bool>,
    /// Letters filter the list: a long one.
    search: Option<Search>,
    adjustments: Option<Vec<protocol::Adjustment>>,
    /// The rows in groups, one listed at a time.
    tabs: Option<Tabs>,
}

impl Select {
    pub(crate) fn new(title: Vec<Line<'static>>, choices: Vec<Choice>, escape: &str) -> Self {
        let search = (choices.len() > WINDOW).then(|| {
            let rows = choices.iter().map(|c| search::keys(&c.label, &c.detail));
            Search::new(rows.collect(), &title)
        });
        Self {
            search,
            title,
            choices,
            escape: escape.into(),
            ..Self::default()
        }
    }

    /// Each original row owns its value even when the search changes its position.
    pub(crate) fn adjustable(mut self, mut adjustments: Vec<protocol::Adjustment>) -> Self {
        adjustments.resize_with(self.choices.len(), Default::default);
        for adjustment in &mut adjustments {
            adjustment.at = adjustment.at.min(adjustment.values.len().saturating_sub(1));
        }
        self.adjustments = Some(adjustments);
        self
    }

    /// The rows in `groups`, as tabs, shown on the marked row's: after [`Select::at`].
    pub(crate) fn grouped(mut self, groups: &[protocol::Group]) -> Self {
        self.tabs = Tabs::new(groups, self.choices.len(), self.at);
        self
    }

    /// The options that match, as positions in the whole list, best first.
    fn matching(&self) -> Vec<usize> {
        match &self.search {
            Some(search) => search.shown.clone(),
            None => (0..self.choices.len()).collect(),
        }
    }

    /// The options listed: those that match, of the tab shown.
    fn rows(&self) -> Vec<usize> {
        let mut rows = self.matching();
        if let Some(tabs) = &self.tabs {
            rows.retain(|at| tabs.holds(*at));
        }
        rows
    }

    /// The marker on the best match of the query, in the tab shown when it has one, else in
    /// the tab of the best.
    fn follow(&mut self) {
        let Some(search) = self.search.as_ref().filter(|search| !search.is_empty()) else {
            return;
        };
        let shown = &search.shown;
        let here = (self.tabs.as_ref()).and_then(|tabs| shown.iter().find(|at| tabs.holds(**at)));
        let Some(&best) = here.or_else(|| shown.first()) else {
            return;
        };
        self.at = best;
        if let Some(tabs) = &mut self.tabs {
            tabs.show(best);
        }
    }

    pub(crate) fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self.checked = vec![false; self.choices.len()];
        self
    }

    /// The marker starts on `at`.
    pub(crate) fn at(mut self, at: usize) -> Self {
        self.at = at.min(self.choices.len().saturating_sub(1));
        self
    }

    pub(crate) fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    /// Title line `at`, if any, names who asks: [`Select::unlabel`] takes it out.
    pub(crate) fn labelled(mut self, at: Option<usize>) -> Self {
        self.label = at.filter(|at| *at < self.title.len());
        self
    }

    /// The title without the line naming who asks, once that is plain.
    pub(crate) fn unlabel(&mut self) {
        if let Some(at) = self.label.take() {
            self.title.remove(at);
        }
    }

    /// Opens the line of the option at `at` at once: a question with nothing to pick from.
    pub(crate) fn typing(mut self, at: usize) -> Self {
        self.at = at;
        self.typing = Some(Composer::masked(self.hidden));
        // With nothing else to pick, the line is the whole question: no list, and Esc cancels.
        self.direct = self.choices.len() == 1;
        self
    }

    /// What a key did: `None` while the choice is still open.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "crossterm's keys, a foreign enum: the others are typed, or do nothing here"
    )]
    pub(crate) fn key(&mut self, key: KeyEvent) -> Option<Picked> {
        if let Some(composer) = &mut self.typing {
            // Ctrl-C is a step back as Esc is: the options again, or a cancel with none.
            let back = key.code == KeyCode::Esc
                || (key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL));
            if back {
                if self.direct {
                    return Some(Picked::Cancelled);
                }
                self.typing = None;
                return None;
            }
            let sent = composer.key(key);
            return sent.map(|text| Picked::Typed(self.at, text.trim().to_string()));
        }
        let toggles = self.multiple && key.code == KeyCode::Char(' ');
        if let Some(search) = self.search.as_mut().filter(|_| !toggles) {
            match search.key(key) {
                Searched::Changed => {
                    self.follow();
                    return None;
                }
                Searched::Cancel => return Some(Picked::Cancelled),
                Searched::Ignored => {}
            }
        }
        if let (Some(tabs), KeyCode::Tab | KeyCode::BackTab) = (&mut self.tabs, key.code) {
            let kept = tabs.step(key.code == KeyCode::Tab, self.at);
            let rows = self.rows();
            let first = rows.first().copied();
            self.at = (kept.filter(|at| rows.contains(at)).or(first)).unwrap_or(self.at);
            return None;
        }
        if matches!(key.code, KeyCode::Left | KeyCode::Right)
            && !self.rows().is_empty()
            && let Some(adjustment) = self
                .adjustments
                .as_mut()
                .and_then(|rows| rows.get_mut(self.at))
        {
            adjustment.at = if key.code == KeyCode::Left {
                adjustment.at.saturating_sub(1)
            } else {
                (adjustment.at + 1).min(adjustment.values.len().saturating_sub(1))
            };
            return None;
        }
        let code = match key.code {
            KeyCode::Char('k') => KeyCode::Up,
            KeyCode::Char('j') => KeyCode::Down,
            code => code,
        };
        if let Some(to) = moved(&self.rows(), self.at, code, WINDOW) {
            self.at = to;
            return None;
        }
        match key.code {
            KeyCode::Esc => return Some(Picked::Cancelled),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Some(Picked::Cancelled);
            }
            KeyCode::Char(' ') if self.multiple && !self.choices[self.at].types => {
                self.checked[self.at] = !self.checked[self.at];
            }
            KeyCode::Enter if self.multiple && self.checked.contains(&true) => {
                let picked = (0..self.checked.len())
                    .filter(|at| self.checked[*at])
                    .collect();
                return Some(Picked::Many(picked));
            }
            KeyCode::Enter if self.rows().is_empty() => {}
            KeyCode::Enter => return self.pick(self.at),
            KeyCode::Char(c) => {
                let numbered = c
                    .to_digit(10)
                    .map(|n| n as usize)
                    .filter(|n| (1..=self.choices.len()).contains(n));
                if let Some(n) = numbered {
                    return self.pick(n - 1);
                }
            }
            _ => {}
        }
        None
    }

    /// Text pasted while typing goes on the line, else on the query; elsewhere it is ignored.
    pub(crate) fn paste(&mut self, text: &str) {
        if let Some(composer) = &mut self.typing {
            composer.paste(text);
        } else if let Some(search) = &mut self.search {
            search.paste(text);
            self.follow();
        }
    }

    fn pick(&mut self, at: usize) -> Option<Picked> {
        self.at = at;
        if self.choices[at].types {
            self.typing = Some(Composer::masked(self.hidden));
            return None;
        }
        match &self.adjustments {
            Some(adjustments) => Some(Picked::Adjusted(at, adjustments[at].at)),
            None => Some(Picked::Option(at)),
        }
    }
}

#[cfg(test)]
#[path = "select_tests.rs"]
mod tests;
