//! Styled lines wrapped to a width by display columns, at word boundaries, with a prefix for the
//! first row and another for the rest (a hanging indent), so a long command or a paragraph never
//! breaks mid-word and its continuation sits under its own start. A word and the punctuation that
//! runs into it, and the words of a code span, keep to one row where a row holds them. A word wider
//! than the row is split; nothing is ever wider than `width`.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

/// A space a row is not broken at while a row holds what it joins: the spaces of code. Drawn as
/// a space, so what is copied from the screen is what was written.
pub(crate) const NO_BREAK: char = '\u{a0}';

/// What follows a word.
#[derive(Clone, Copy, PartialEq)]
enum After {
    /// A space, where a row may end.
    Space,
    /// A space that keeps its words on one row when a row holds them.
    Joint,
    /// The next word, with nothing between: "`code`," or "a**b**".
    Nothing,
}

/// The columns `text` takes.
pub(crate) fn width(text: &str) -> usize {
    text.chars().map(|c| c.width().unwrap_or(0)).sum()
}

pub(crate) fn line_width(line: &Line) -> usize {
    line.spans.iter().map(|span| width(&span.content)).sum()
}

/// `line` wrapped to `width` columns: rows start with `first`, then `rest`.
pub(crate) fn wrap(
    line: &Line<'static>,
    width: usize,
    first: &Line<'static>,
    rest: &Line<'static>,
) -> Vec<Line<'static>> {
    let mut rows: Vec<Line<'static>> = vec![];
    let mut row = first.clone();
    let mut used = line_width(first);
    let indent = line_width(rest);
    let room = width.max(indent + 1);
    let mut pending_space: Option<Style> = None;
    let mut fresh = true;
    let words = words(line);
    let runs = runs(&words);
    for ((mut word, style, after, space), run) in words.into_iter().zip(runs) {
        // A word and what runs into it ("`code`," or "(**b**)"), and the words of a code span,
        // go to the next row together when a row holds them: no row starts with the comma.
        let gap = usize::from(pending_space.is_some());
        if !fresh && used + gap + run > room && indent + run <= room {
            rows.push(std::mem::replace(&mut row, rest.clone()));
            used = indent;
            fresh = true;
        }
        loop {
            let gap = usize::from(pending_space.is_some() && !fresh);
            let w = self::width(&word);
            if used + gap + w <= room {
                if gap == 1 {
                    row.spans
                        .push(Span::styled(" ", pending_space.unwrap_or_default()));
                }
                row.spans.push(Span::styled(word, style));
                used += gap + w;
                fresh = false;
                break;
            }
            if !fresh {
                rows.push(std::mem::replace(&mut row, rest.clone()));
                used = indent;
                fresh = true;
                continue;
            }
            let tail = word.split_off(split(&word, room.saturating_sub(used).max(1)));
            row.spans
                .push(Span::styled(std::mem::replace(&mut word, tail), style));
            rows.push(std::mem::replace(&mut row, rest.clone()));
            used = indent;
            if word.is_empty() {
                break;
            }
        }
        pending_space = (after != After::Nothing).then_some(space);
    }
    if !fresh || rows.is_empty() {
        rows.push(row);
    }
    rows
}

/// A word, its style, what follows it, and the style of the space that does: the span's the
/// space is written in, so a link's underline ends with its last word.
type Word = (String, Style, After, Style);

/// The words of `line`, and what follows each.
fn words(line: &Line) -> Vec<Word> {
    let mut out: Vec<Word> = vec![];
    for span in &line.spans {
        let style = line.style.patch(span.style);
        let mut word = String::new();
        for c in span.content.chars() {
            if c.is_whitespace() {
                let after = if c == NO_BREAK {
                    After::Joint
                } else {
                    After::Space
                };
                if !word.is_empty() {
                    out.push((std::mem::take(&mut word), style, after, style));
                } else if let Some(last) = out.last_mut() {
                    (last.2, last.3) = (after, style);
                }
            } else {
                word.push(c);
            }
        }
        if !word.is_empty() {
            out.push((word, style, After::Nothing, style));
        }
    }
    out
}

/// For each word, the columns kept on one row from it on: the words that run into it, and where
/// a code span starts, the span. A span no row holds is then broken at its spaces, as words are.
fn runs(words: &[Word]) -> Vec<usize> {
    let mut runs = vec![0; words.len()];
    let (mut glued, mut whole) = (0, 0);
    for (at, (word, _, after, _)) in words.iter().enumerate().rev() {
        let width = width(word);
        (glued, whole) = match after {
            After::Space => (width, width),
            After::Joint => (width, width + 1 + whole),
            After::Nothing => (width + glued, width + whole),
        };
        let before = at.checked_sub(1).and_then(|at| words.get(at));
        runs[at] = match before.map(|word| word.2) {
            None | Some(After::Space) => whole,
            Some(After::Joint) => glued,
            Some(After::Nothing) => 0,
        };
    }
    runs
}

/// A line of code with each tab run out to the next fourth column: a terminal's own stops are
/// eight wide, and an escaped tab is not what was written.
pub(crate) fn untabbed(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut column = 0;
    for (at, piece) in line.split('\t').enumerate() {
        if at > 0 {
            let pad = 4 - column % 4;
            out.extend(std::iter::repeat_n(' ', pad));
            column += pad;
        }
        out.push_str(piece);
        column += width(piece);
    }
    out
}

/// The byte index where `word` is cut to fit `columns`: its length when it fits.
fn split(word: &str, columns: usize) -> usize {
    let mut used = 0;
    for (at, c) in word.char_indices() {
        let w = c.width().unwrap_or(0);
        if used + w > columns && at > 0 {
            return at;
        }
        used += w;
    }
    word.len()
}

/// `text` in rows of at most `columns`, broken anywhere and keeping its spaces: for code, whose
/// indentation and alignment mean something.
pub(crate) fn hard(text: &str, columns: usize) -> Vec<String> {
    let columns = columns.max(1);
    let mut rows = vec![String::new()];
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > columns && rows.last().is_some_and(|row| !row.is_empty()) {
            rows.push(String::new());
            used = 0;
        }
        if let Some(row) = rows.last_mut() {
            row.push(c);
        }
        used += w;
    }
    rows
}

/// `line` in rows of at most `columns`, broken anywhere with its styles kept: what the terminal
/// would do to a row too wide, done first so the rows drawn are the rows counted.
pub(crate) fn fit(line: Line<'static>, columns: usize) -> Vec<Line<'static>> {
    let columns = columns.max(1);
    if line_width(&line) <= columns {
        return vec![line];
    }
    let mut rows = vec![Line::default()];
    let mut used = 0;
    for span in &line.spans {
        let style = line.style.patch(span.style);
        let mut text = String::new();
        for c in span.content.chars() {
            let w = c.width().unwrap_or(0);
            if used + w > columns {
                if let Some(row) = rows.last_mut() {
                    row.spans
                        .push(Span::styled(std::mem::take(&mut text), style));
                }
                rows.push(Line::default());
                used = 0;
            }
            text.push(c);
            used += w;
        }
        if let Some(row) = rows.last_mut() {
            row.spans.push(Span::styled(text, style));
        }
    }
    rows
}

/// `text` cut to `columns`, ending in `…` when cut.
pub(crate) fn cut(text: &str, columns: usize) -> String {
    if width(text) <= columns {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w + 1 > columns {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

#[cfg(test)]
#[path = "wrap_tests.rs"]
mod tests;
