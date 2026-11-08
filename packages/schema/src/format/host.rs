//! Hostnames (RFC 1123, with RFC 5891's A-labels) and email addresses (RFC 5321's `Mailbox`).

use super::idna;
use super::ip::{ipv4, ipv6};

/// Labels of ASCII letters, digits and hyphens joined by dots: each 1 to 63 long, starting and
/// ending with no hyphen, 253 in all, no dot at the end. A label with hyphens third and fourth is
/// an A-label (`xn--`), whose Punycode must decode to a valid U-label.
pub(super) fn hostname(text: &[u8]) -> bool {
    !text.is_empty() && text.len() <= 253 && text.split(|&c| c == b'.').all(label)
}

/// A hostname as RFC 1123 alone reads it, as drafts 4 and 6 do: no rule for an A-label.
pub(super) fn rfc1123(text: &[u8]) -> bool {
    !text.is_empty() && text.len() <= 253 && text.split(|&c| c == b'.').all(plain)
}

/// One label of RFC 1123: letters, digits and hyphens, 1 to 63, no hyphen at either end.
fn plain(text: &[u8]) -> bool {
    let (Some(&first), Some(&last)) = (text.first(), text.last()) else {
        return false;
    };
    text.len() <= 63
        && first != b'-'
        && last != b'-'
        && text.iter().all(|&c| c.is_ascii_alphanumeric() || c == b'-')
}

/// One label of a hostname.
fn label(text: &[u8]) -> bool {
    if !plain(text) {
        return false;
    }
    match text {
        [x, n, b'-', b'-', payload @ ..]
            if x.eq_ignore_ascii_case(&b'x') && n.eq_ignore_ascii_case(&b'n') =>
        {
            idna::a_label(payload)
        }
        [_, _, b'-', b'-', ..] => false,
        _ => true,
    }
}

/// `local@domain`: the local part dot-separated atoms or a quoted string, at most 64 long; the
/// domain a hostname, or an IPv4 or IPv6 address in brackets.
pub(super) fn email(text: &[u8]) -> bool {
    let Some(domain) = after_local(text) else {
        return false;
    };
    match domain {
        [b'[', literal @ .., b']'] => match literal.split_at_checked(5) {
            Some((tag, address)) if tag.eq_ignore_ascii_case(b"IPv6:") => ipv6(address),
            _ => ipv4(literal),
        },
        _ => hostname(domain),
    }
}

/// What follows the local part's `@`, `None` when there is no valid local part and `@`.
fn after_local(text: &[u8]) -> Option<&[u8]> {
    let end = match text.first() {
        Some(b'"') => quoted_end(text)?,
        _ => {
            let at = text.iter().position(|&c| c == b'@')?;
            let atoms = text.get(..at)?;
            let dot_string = atoms
                .split(|&c| c == b'.')
                .all(|atom| !atom.is_empty() && atom.iter().all(|&c| atext(c)));
            dot_string.then_some(at)?
        }
    };
    if end > 64 {
        return None;
    }
    text.get(end..)?.strip_prefix(b"@")
}

/// Where a quoted local part ends, after its closing quote: printable ASCII inside, a quote or a
/// backslash escaped by a backslash.
fn quoted_end(text: &[u8]) -> Option<usize> {
    let mut at = 1;
    loop {
        match *text.get(at)? {
            b'"' => return Some(at + 1),
            b'\\' => {
                let escaped = *text.get(at + 1)?;
                (b' '..=b'~').contains(&escaped).then_some(())?;
                at += 2;
            }
            b' '..=b'~' => at += 1,
            _ => return None,
        }
    }
}

/// RFC 5321's `atext`: what an atom of the local part is made of.
fn atext(c: u8) -> bool {
    c.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&c)
}
