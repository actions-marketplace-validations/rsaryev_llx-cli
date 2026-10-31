//! Type-to-search in a long list: letters go to a query, and the list shows the rows that match
//! it, best first. A row is found by its label and the lead of its detail (the part before
//! ` · `, where a list puts an id), never by the notes after it: "signed" must not find every
//! provider signed in to.
//!
//! ```text
//!   Search: open▏  2 matches
//!
//! ❯ OpenAI openai · not signed in
//!   OpenRouter openrouter · signed in: OPENROUTER_API_KEY
//! ```

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::text::{Line, Span};

use super::style;
use crate::printable::printable;

/// What a row is found by, lower-cased: its label, and its detail's lead.
pub(super) fn keys(label: &str, detail: &str) -> [String; 2] {
    let lead = detail.split(" · ").next().unwrap_or_default();
    [label.to_lowercase(), lead.to_lowercase()]
}

/// The rows that match `query`, best first: a key that starts with it, then one that holds it,
/// then one whose words start with its letters in turn ("gas" finds "Google AI Studio"); rows of
/// the same rank keep their order. Every row, in order, for an empty query.
fn ranked(query: &str, rows: &[[String; 2]]) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return (0..rows.len()).collect();
    }
    let rank = |keys: &[String; 2]| {
        let tiers = [
            keys.iter().any(|key| key.starts_with(&query)),
            keys.iter().any(|key| key.contains(&query)),
            keys.iter().any(|key| initials(&query, key)),
        ];
        tiers.iter().position(|hit| *hit)
    };
    let mut found: Vec<(usize, usize)> = rows
        .iter()
        .enumerate()
        .filter_map(|(at, keys)| rank(keys).map(|tier| (tier, at)))
        .collect();
    // No two are equal (`at` is each row's own), so an unstable sort keeps the same order.
    found.sort_unstable();
    found.into_iter().map(|(_, at)| at).collect()
}

/// Each letter of `query` starts one of the words of `key`, in order.
fn initials(query: &str, key: &str) -> bool {
    let mut starts = key
        .split(|c: char| !c.is_alphanumeric())
        .filter_map(|word| word.chars().next());
    query
        .chars()
        .filter(|c| c.is_alphanumeric())
        .all(|letter| starts.any(|start| start == letter))
}

/// A list's query, and the rows that match it.
pub(super) struct Search {
    query: String,
    keys: Vec<[String; 2]>,
    /// The rows shown, best first.
    pub(super) shown: Vec<usize>,
    /// What the list holds, for "No provider matches": its title's first word.
    noun: String,
}

/// What a key did to the search.
pub(super) enum Searched {
    /// The query changed: the marker goes to the best match.
    Changed,
    /// Esc on an empty query: the list is cancelled.
    Cancel,
    /// Not a key the search takes.
    Ignored,
}

impl Search {
    pub(super) fn new(rows: Vec<[String; 2]>, title: &[Line<'static>]) -> Self {
        let heading: String = title
            .first()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .unwrap_or_default();
        // A list in steps (`Login › Provider`) holds what its last step names.
        let heading = heading.rsplit('›').next().unwrap_or_default();
        let heading = heading.trim_start_matches(['•', ' ']);
        let noun = heading.split([' ', '(']).next().unwrap_or_default();
        Self {
            query: String::new(),
            shown: (0..rows.len()).collect(),
            keys: rows,
            noun: match noun.is_empty() {
                true => "option".into(),
                false => noun.to_lowercase(),
            },
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.query.is_empty()
    }

    /// Letters (digits and signs too) add to the query, Backspace takes one off, Esc and Ctrl-C
    /// clear it (on an empty one, Esc cancels and Ctrl-C is the list's), Ctrl-U clears it.
    pub(super) fn key(&mut self, key: KeyEvent) -> Searched {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let plain = !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "crossterm's keys, a foreign enum: the others leave the query"
        )]
        match key.code {
            KeyCode::Char('c' | 'u') if ctrl && !self.query.is_empty() => self.query.clear(),
            KeyCode::Char(c) if plain => self.query.push(c),
            KeyCode::Backspace if self.query.pop().is_some() => {}
            KeyCode::Esc if self.query.is_empty() => return Searched::Cancel,
            KeyCode::Esc => self.query.clear(),
            _ => return Searched::Ignored,
        }
        self.shown = ranked(&self.query, &self.keys);
        Searched::Changed
    }

    /// Pasted text goes on the query.
    pub(super) fn paste(&mut self, text: &str) {
        self.query.extend(text.chars().filter(|c| !c.is_control()));
        self.shown = ranked(&self.query, &self.keys);
    }

    /// `Search: open▏  2 matches`, and a line when `none` is listed (none matches, or none in
    /// the tab shown).
    pub(super) fn render(&self, none: bool) -> Vec<Line<'static>> {
        let count = match (self.query.is_empty(), self.shown.len()) {
            (true, all) => format!("{all} in all"),
            (false, 1) => "1 match".into(),
            (false, n) => format!("{n} matches"),
        };
        let mut out = vec![Line::from(vec![
            Span::raw("  Search: "),
            Span::styled(printable(&self.query), style::bold()),
            Span::raw("▏  "),
            Span::styled(count, style::dim()),
        ])];
        out.push(Line::default());
        if none {
            let query = printable(&self.query);
            let none = format!("  No {} matches '{query}'", self.noun);
            out.push(Line::from(Span::styled(none, style::dim())));
        }
        out
    }
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
