//! Styled lines wrapped to a width by display columns, at word boundaries, with a prefix for the
//! first row and another for the rest (a hanging indent), so a long command or a paragraph never
//! breaks mid-word and its continuation sits under its own start. A word wider than the row is
//! split; nothing is ever wider than `width`.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

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
    for (mut word, style, space_after) in words(line) {
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
        pending_space = space_after.then_some(style);
    }
    if !fresh || rows.is_empty() {
        rows.push(row);
    }
    rows
}

/// The words of `line` with their style and whether a space followed them.
fn words(line: &Line) -> Vec<(String, Style, bool)> {
    let mut out: Vec<(String, Style, bool)> = vec![];
    for span in &line.spans {
        let style = line.style.patch(span.style);
        let mut word = String::new();
        for c in span.content.chars() {
            if c.is_whitespace() {
                if !word.is_empty() {
                    out.push((std::mem::take(&mut word), style, true));
                } else if let Some(last) = out.last_mut() {
                    last.2 = true;
                }
            } else {
                word.push(c);
            }
        }
        if !word.is_empty() {
            // A word that runs into the next span ("`code`," or "a**b**") stays joined to it.
            out.push((word, style, false));
        }
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
