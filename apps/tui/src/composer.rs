//! Where the person types: a few lines of text with a cursor, the earlier entries a key away, and
//! the marks that stand for what is attached or pasted (`marks.rs`). What `/` and `@` offer is
//! the keys' (`keys.rs`).
//!
//! Enter sends; Shift+Enter, Alt+Enter or Ctrl+J start a new line, as does a line ending in `\`;
//! a paste keeps its lines. Left and right move by character, with Alt (or Ctrl) by word; Home,
//! End, Ctrl+A and Ctrl+E go to the line's ends; Up and Down move between lines, and past the
//! first or the last one walk the earlier entries; Ctrl+R finds an earlier one with the words
//! typed. Ctrl+U, Ctrl+K and Ctrl+W delete to the start, to the end and the word before. Shift
//! with Left, Right, Home or End selects; Backspace or Delete takes the selection away, and what
//! is typed or pasted replaces it.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::attach::Dropped;

mod marks;
mod render;

use marks::{cleaned, unpasted};

#[derive(Default)]
pub(crate) struct Composer {
    text: Vec<char>,
    /// An index into `text`.
    at: usize,
    /// The fixed end of a keyboard selection, in character indices.
    anchor: Option<usize>,
    /// What was sent before, oldest first.
    earlier: Vec<String>,
    /// While walking `earlier`: which one is shown, and what was typed before the walk began.
    walking: Option<(usize, String)>,
    /// A secret: each character is drawn as a dot.
    masked: bool,
    /// The pictures attached, each with the mark that stands for it in the text.
    attached: Vec<(String, Dropped)>,
    /// Clipboard text hidden behind a mark until the entry is sent.
    pastes: Vec<(String, String)>,
}

impl Composer {
    pub(crate) fn masked(masked: bool) -> Self {
        Self {
            masked,
            ..Self::default()
        }
    }

    pub(crate) fn text(&self) -> String {
        self.text.iter().collect()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.set("");
    }

    /// Clear what is typed, kept among the earlier entries so that ↑ brings it back.
    pub(crate) fn discard(&mut self) {
        let text = self.text();
        self.remember(&text);
        self.walking = None;
        self.clear();
    }

    /// `entry`, as the person would see it again, among the earlier entries unless it is blank
    /// or the last one.
    fn remember(&mut self, entry: &str) {
        let remembered = unpasted(entry, &self.pastes);
        if !remembered.trim().is_empty() && self.earlier.last() != Some(&remembered) {
            self.earlier.push(remembered);
        }
    }

    pub(crate) fn set(&mut self, text: &str) {
        self.text = text.chars().collect();
        self.at = self.text.len();
        self.anchor = None;
    }

    /// The `@` word the cursor is at the end of: where it starts, and what follows the `@`.
    pub(crate) fn mention(&self) -> Option<(usize, String)> {
        let start = self.text[..self.at]
            .iter()
            .rposition(|c| c.is_whitespace())
            .map_or(0, |at| at + 1);
        let word: String = self.text[start..self.at].iter().collect();
        word.strip_prefix('@').map(|rest| (start, rest.to_string()))
    }

    /// The text from `from` to the cursor replaced with `with`.
    pub(crate) fn replace(&mut self, from: usize, with: &str) {
        self.delete(from, self.at);
        self.insert_str(with);
    }

    /// Text that arrived as one paste: its lines are kept, nothing is sent.
    pub(crate) fn paste(&mut self, text: &str) {
        self.insert_str(&cleaned(text));
    }

    /// The text, when the key sent it; else the key edited it, or was not one the composer takes.
    pub(crate) fn key(&mut self, key: KeyEvent) -> Option<String> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        if matches!(
            key.code,
            KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End
        ) {
            if shift {
                self.anchor.get_or_insert(self.at);
            } else {
                self.anchor = None;
            }
        } else if matches!(key.code, KeyCode::Up | KeyCode::Down)
            || (ctrl && matches!(key.code, KeyCode::Char('a' | 'e')))
            || (alt && matches!(key.code, KeyCode::Char('b' | 'f')))
        {
            self.anchor = None;
        }
        if matches!(key.code, KeyCode::Backspace | KeyCode::Delete) && self.delete_selection() {
            return None;
        }
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "crossterm's keys, a foreign enum: the others edit nothing"
        )]
        match key.code {
            KeyCode::Enter if shift || alt => self.insert('\n'),
            KeyCode::Char('j') if ctrl => self.insert('\n'),
            KeyCode::Enter => return self.send(),
            KeyCode::Char('a') if ctrl => self.at = self.line_start(),
            KeyCode::Char('e') if ctrl => self.at = self.line_end(),
            KeyCode::Char('u') if ctrl => self.delete(self.line_start(), self.at),
            KeyCode::Char('k') if ctrl => self.delete(self.at, self.line_end()),
            KeyCode::Char('w') if ctrl => self.delete(self.word_back(), self.at),
            KeyCode::Char('r') if ctrl => self.search_back(),
            KeyCode::Char('b') if alt => self.at = self.word_back(),
            KeyCode::Char('f') if alt => self.at = self.word_forward(),
            KeyCode::Char(_) if ctrl => return None,
            KeyCode::Char(c) => self.insert(c),
            KeyCode::Backspace if alt || ctrl => self.delete(self.word_back(), self.at),
            KeyCode::Backspace => self.delete(self.at.saturating_sub(1), self.at),
            KeyCode::Delete => self.delete(self.at, (self.at + 1).min(self.text.len())),
            KeyCode::Left if alt || ctrl => self.at = self.word_back(),
            KeyCode::Right if alt || ctrl => self.at = self.word_forward(),
            KeyCode::Left => self.at = self.at.saturating_sub(1),
            KeyCode::Right => self.at = (self.at + 1).min(self.text.len()),
            KeyCode::Home => self.at = self.line_start(),
            KeyCode::End => self.at = self.line_end(),
            KeyCode::Up => self.up(),
            KeyCode::Down => self.down(),
            _ => return None,
        }
        None
    }

    fn send(&mut self) -> Option<String> {
        // A line ending in `\` goes on on the next, as in a shell.
        if self.at == self.text.len() && self.text.last() == Some(&'\\') {
            self.text.pop();
            self.at -= 1;
            self.insert('\n');
            return None;
        }
        let text = self.text();
        self.clear();
        self.walking = None;
        self.remember(&text);
        Some(text)
    }

    fn selection(&self) -> Option<std::ops::Range<usize>> {
        self.anchor
            .map(|anchor| anchor.min(self.text.len()))
            .filter(|anchor| *anchor != self.at)
            .map(|anchor| anchor.min(self.at)..anchor.max(self.at))
    }

    fn delete_selection(&mut self) -> bool {
        let selection = self.selection();
        self.anchor = None;
        if let Some(range) = selection {
            self.delete(range.start, range.end);
            true
        } else {
            false
        }
    }

    fn insert(&mut self, c: char) {
        self.delete_selection();
        self.text.insert(self.at, c);
        self.at += 1;
    }

    /// `text` at the cursor in one move, replacing the selection.
    fn insert_str(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.delete_selection();
        let len = text.chars().count();
        self.text.splice(self.at..self.at, text.chars());
        self.at += len;
    }

    fn delete(&mut self, from: usize, to: usize) {
        self.anchor = None;
        if from < to {
            self.text.drain(from..to);
            self.at = from;
        }
    }

    fn line_start(&self) -> usize {
        self.line_start_at(self.at)
    }

    fn line_end(&self) -> usize {
        self.line_end_at(self.at)
    }

    /// Where the line holding index `i` starts.
    fn line_start_at(&self, i: usize) -> usize {
        self.text[..i]
            .iter()
            .rposition(|c| *c == '\n')
            .map_or(0, |at| at + 1)
    }

    /// Where the line holding index `i` ends.
    fn line_end_at(&self, i: usize) -> usize {
        self.text[i..]
            .iter()
            .position(|c| *c == '\n')
            .map_or(self.text.len(), |at| i + at)
    }

    /// The `at`th earlier entry in the box, the cursor at its end.
    fn recall(&mut self, at: usize) {
        self.text = self.earlier[at].chars().collect();
        self.at = self.text.len();
        self.anchor = None;
    }

    fn word_back(&self) -> usize {
        let mut at = self.at;
        while at > 0 && self.text[at - 1].is_whitespace() {
            at -= 1;
        }
        while at > 0 && !self.text[at - 1].is_whitespace() {
            at -= 1;
        }
        at
    }

    fn word_forward(&self) -> usize {
        let mut at = self.at;
        while at < self.text.len() && self.text[at].is_whitespace() {
            at += 1;
        }
        while at < self.text.len() && !self.text[at].is_whitespace() {
            at += 1;
        }
        at
    }

    /// Ctrl-R: the next earlier entry with the words typed before the search in it, as a
    /// shell's reverse search finds them; again for the one before.
    fn search_back(&mut self) {
        let (from, typed) =
            (self.walking.take()).unwrap_or_else(|| (self.earlier.len(), self.text()));
        let found = self.earlier[..from]
            .iter()
            .rposition(|entry| entry.contains(typed.trim()));
        match found {
            Some(at) => {
                self.recall(at);
                self.walking = Some((at, typed));
            }
            // Nothing earlier: the entry found last stays, and so does the search.
            None if from < self.earlier.len() => self.walking = Some((from, typed)),
            None => {}
        }
    }

    fn up(&mut self) {
        let start = self.line_start();
        if start > 0 {
            let column = self.at - start;
            self.at = (self.line_start_at(start - 1) + column).min(start - 1);
            return;
        }
        let next = match &self.walking {
            Some((0, _)) => return,
            Some((at, _)) => at - 1,
            None if self.earlier.is_empty() => return,
            None => self.earlier.len() - 1,
        };
        let typed = self
            .walking
            .take()
            .map_or_else(|| self.text(), |(_, typed)| typed);
        self.recall(next);
        self.walking = Some((next, typed));
    }

    fn down(&mut self) {
        let end = self.line_end();
        if end < self.text.len() {
            let column = self.at - self.line_start();
            self.at = (end + 1 + column).min(self.line_end_at(end + 1));
            return;
        }
        match self.walking.take() {
            Some((at, typed)) if at + 1 < self.earlier.len() => {
                self.recall(at + 1);
                self.walking = Some((at + 1, typed));
            }
            Some((_, typed)) => self.set(&typed),
            None => {}
        }
    }
}

#[cfg(test)]
#[path = "composer_tests.rs"]
mod tests;
