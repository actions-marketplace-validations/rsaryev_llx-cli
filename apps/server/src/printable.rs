//! Text from the model or a tool, made safe and flat where the server shows it itself (an approval at the
//! terminal, a session's title and list): control characters and reordering marks are escaped.
//! The interface keeps its own (`tui::printable`): it knows nothing of the server (L7).

use unicode_width::UnicodeWidthChar;

/// `text` on one line: its runs of white space (newlines too) each one space, its ends bare.
pub(crate) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `text`'s first `chars` characters, with `…` when that left something out.
pub(crate) fn clipped(text: &str, chars: usize) -> String {
    let mut shown: String = text.chars().take(chars).collect();
    if shown.len() < text.len() {
        shown.push('…');
    }
    shown
}

/// `text` cut to `columns` as a terminal counts them (a wide character takes two), ending in `…`
/// when cut: a saved step's detail is cut so, and its bytes stay as they were.
pub(crate) fn cut(text: &str, columns: usize) -> String {
    let width = |c: char| c.width().unwrap_or(0);
    if text.chars().map(width).sum::<usize>() <= columns {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = width(c);
        if used + w + 1 > columns {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

/// `text` with every control character and every reordering mark escaped.
pub(crate) fn printable(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let reorders = matches!(
            c,
            '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
        );
        match c.is_control() || reorders {
            true => out.extend(c.escape_default()),
            false => out.push(c),
        }
    }
    out
}

#[cfg(test)]
#[path = "printable_tests.rs"]
mod tests;
