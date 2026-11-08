//! URI templates (RFC 6570, section 2): literals, and expressions in braces.

use super::{code_point, iprivate, percent_encoded, ucschar};

/// Literals and `{expressions}`, each expression an optional operator and one variable or more,
/// separated by commas.
pub(super) fn uri_template(text: &[u8]) -> bool {
    let mut rest = text;
    loop {
        while let Some((&byte, after)) = rest.split_first()
            && LITERALS[usize::from(byte)]
        {
            rest = after;
        }
        let Some((&byte, after)) = rest.split_first() else {
            return true;
        };
        rest = match byte {
            b'{' => match expression(after) {
                Some(after) => after,
                None => return false,
            },
            b'%' if percent_encoded(rest) => rest.get(3..).unwrap_or_default(),
            0x80..=0xFF => match code_point(rest) {
                Some((point, length)) if ucschar(point) || iprivate(point) => {
                    rest.get(length..).unwrap_or_default()
                }
                _ => return false,
            },
            _ => return false,
        };
    }
}

/// The ASCII literals (with the apostrophe RFC 6570's erratum 6937 admits); the bytes beyond
/// ASCII are read as code points.
const LITERALS: [bool; 256] = {
    let mut table = [false; 256];
    let mut byte: u8 = 0x21;
    while byte < 0x7F {
        table[byte as usize] = !matches!(
            byte,
            b'"' | b'%' | b'<' | b'>' | b'\\' | b'^' | b'`' | b'{' | b'|' | b'}'
        );
        byte += 1;
    }
    table
};

/// What follows an expression's `}`, `None` when it is not one.
fn expression(text: &[u8]) -> Option<&[u8]> {
    let mut rest = match text.split_first() {
        Some((
            b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&' | b'=' | b',' | b'!' | b'@' | b'|',
            after,
        )) => after,
        _ => text,
    };
    loop {
        rest = modifier(variable(rest)?)?;
        match rest.split_first()? {
            (b',', after) => rest = after,
            (b'}', after) => return Some(after),
            _ => return None,
        }
    }
}

/// What follows a variable's name: characters, joined by single dots.
fn variable(text: &[u8]) -> Option<&[u8]> {
    let mut rest = text;
    loop {
        rest = match rest.split_first() {
            Some((b'_' | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z', after)) => after,
            Some((b'%', _)) if percent_encoded(rest) => rest.get(3..)?,
            _ => return None,
        };
        // More characters, or a dot before one.
        rest = match rest.split_first() {
            Some((b'.', after)) => after,
            Some((b'_' | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'%', _)) => rest,
            _ => return Some(rest),
        };
    }
}

/// What follows a variable's modifier: `*`, or `:` and a length of 1 to 9999, or none.
fn modifier(text: &[u8]) -> Option<&[u8]> {
    match text.split_first() {
        Some((b'*', after)) => Some(after),
        Some((b':', after)) => {
            let digits = after.iter().take_while(|c| c.is_ascii_digit()).count();
            let starts_well = after.first().is_some_and(|&c| c != b'0');
            (starts_well && (1..=4).contains(&digits)).then(|| after.get(digits..))?
        }
        _ => Some(text),
    }
}
