//! The parts of a link, as an inline link and a reference definition both write them: a label,
//! a destination, a title.

use crate::scan::{is_ascii_punct, is_space};

/// A part of a link: its text, and where what follows it starts.
#[derive(Clone, Copy)]
pub(crate) struct Part {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) after: usize,
}

fn escape(text: &[u8], at: usize) -> bool {
    text.get(at) == Some(&b'\\') && text.get(at + 1).is_some_and(|byte| is_ascii_punct(*byte))
}

/// The label that starts at `text[at]` (a `[`): where it ends, past its `]`.
pub(crate) fn label(text: &[u8], at: usize) -> Option<usize> {
    if text.get(at) != Some(&b'[') {
        return None;
    }
    let mut end = at + 1;
    loop {
        match *text.get(end)? {
            b']' => return Some(end + 1),
            b'[' => return None,
            b'\\' => end += 2,
            _ => end += 1,
        }
        if end - at > 1000 {
            return None;
        }
    }
}

pub(crate) fn destination(text: &[u8], at: usize) -> Option<Part> {
    if text.get(at) == Some(&b'<') {
        let mut end = at + 1;
        loop {
            match *text.get(end)? {
                b'>' => {
                    return Some(Part {
                        start: at + 1,
                        end,
                        after: end + 1,
                    });
                }
                b'<' | b'\n' => return None,
                _ if escape(text, end) => end += 2,
                _ => end += 1,
            }
        }
    }
    let mut depth = 0;
    let mut end = at;
    while let Some(byte) = text.get(end) {
        match byte {
            _ if escape(text, end) => end += 1,
            // Deeper than any address goes: read no further, or text of `(` alone is read
            // again from each one.
            b'(' if depth == 32 => return None,
            b'(' => depth += 1,
            b')' if depth == 0 => break,
            b')' => depth -= 1,
            0..=b' ' => break,
            _ => {}
        }
        end += 1;
    }
    if depth != 0 || (end == at && text.get(end) != Some(&b')')) {
        return None;
    }
    Some(Part {
        start: at,
        end,
        after: end,
    })
}

pub(crate) fn title(text: &[u8], at: usize) -> Option<Part> {
    let close = match *text.get(at)? {
        b'"' => b'"',
        b'\'' => b'\'',
        b'(' => b')',
        _ => return None,
    };
    let mut end = at + 1;
    loop {
        match *text.get(end)? {
            byte if byte == close => {
                return Some(Part {
                    start: at + 1,
                    end,
                    after: end + 1,
                });
            }
            b'(' if close == b')' => return None,
            _ if escape(text, end) => end += 2,
            _ => end += 1,
        }
    }
}

/// Past spaces and tabs, and one line ending among them.
pub(crate) fn spnl(text: &[u8], mut at: usize) -> usize {
    let mut lines = 0;
    while let Some(byte) = text.get(at) {
        if *byte == b'\n' && lines == 0 {
            lines = 1;
        } else if !is_space(*byte) {
            break;
        }
        at += 1;
    }
    at
}
