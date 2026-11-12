//! HTML as CommonMark reads it inside a line: a tag, a comment, an instruction, a declaration,
//! a CDATA section. Nothing is checked beyond its shape.

use crate::scan::is_blank;

/// Where `pattern` starts in `text`, from `from` on.
fn seek(text: &[u8], from: usize, pattern: &[u8]) -> Option<usize> {
    text.get(from..)?
        .windows(pattern.len())
        .position(|window| window == pattern)
        .map(|at| at + from)
}

fn blanks(text: &[u8], mut at: usize) -> usize {
    while text.get(at).is_some_and(|byte| is_blank(*byte)) {
        at += 1;
    }
    at
}

/// A tag's name from `at`: where it ends.
fn name(text: &[u8], at: usize) -> Option<usize> {
    if !text.get(at)?.is_ascii_alphabetic() {
        return None;
    }
    let mut end = at + 1;
    while text
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
    {
        end += 1;
    }
    Some(end)
}

/// An attribute's value from `at`: where it ends.
fn value(text: &[u8], at: usize) -> Option<usize> {
    let first = *text.get(at)?;
    if first == b'"' || first == b'\'' {
        let close = text.get(at + 1..)?.iter().position(|byte| *byte == first)?;
        return Some(at + close + 2);
    }
    let mut end = at;
    while text.get(end).is_some_and(|byte| {
        !is_blank(*byte) && !matches!(byte, b'"' | b'\'' | b'=' | b'<' | b'>' | b'`')
    }) {
        end += 1;
    }
    (end > at).then_some(end)
}

/// The attributes from `at`, each after whitespace: where the last one ends.
fn attributes(text: &[u8], mut at: usize) -> usize {
    loop {
        let start = blanks(text, at);
        let named = text
            .get(start)
            .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b':'));
        if start == at || !named {
            return at;
        }
        let mut end = start + 1;
        while text.get(end).is_some_and(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b':' | b'-')
        }) {
            end += 1;
        }
        let equals = blanks(text, end);
        if text.get(equals) == Some(&b'=') {
            match value(text, blanks(text, equals + 1)) {
                Some(after) => end = after,
                None => return at,
            }
        }
        at = end;
    }
}

/// The open or closing tag that starts at `text[at]` (a `<`): where it ends.
pub(crate) fn tag(text: &[u8], at: usize) -> Option<usize> {
    let closing = text.get(at + 1) == Some(&b'/');
    let mut end = name(text, at + 1 + usize::from(closing))?;
    if !closing {
        end = attributes(text, end);
    }
    end = blanks(text, end);
    if !closing && text.get(end) == Some(&b'/') {
        end += 1;
    }
    (text.get(end) == Some(&b'>')).then_some(end + 1)
}

/// The HTML that starts at `text[at]` (a `<`): where it ends.
pub(crate) fn html(text: &[u8], at: usize) -> Option<usize> {
    let rest = text.get(at + 1..)?;
    if rest.starts_with(b"!--") {
        if rest.starts_with(b"!-->") {
            return Some(at + 5);
        }
        if rest.starts_with(b"!--->") {
            return Some(at + 6);
        }
        return seek(text, at + 4, b"-->").map(|end| end + 3);
    }
    if rest.starts_with(b"?") {
        return seek(text, at + 2, b"?>").map(|end| end + 2);
    }
    if rest.starts_with(b"![CDATA[") {
        return seek(text, at + 9, b"]]>").map(|end| end + 3);
    }
    if rest.starts_with(b"!") && rest.get(1).is_some_and(u8::is_ascii_alphabetic) {
        return seek(text, at + 2, b">").map(|end| end + 1);
    }
    tag(text, at)
}

#[cfg(test)]
#[path = "tag_tests.rs"]
mod tests;
