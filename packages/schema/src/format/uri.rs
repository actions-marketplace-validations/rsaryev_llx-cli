//! URIs (RFC 3986) and IRIs (RFC 3987), absolute or references, read in one pass: each part
//! scanned until a byte its class does not take, which must be the next part's delimiter. ASCII
//! goes through a table of classes, an IRI's code points beyond it by range.

use super::ip::ipv6;
use super::{code_point, iprivate, percent_encoded, ucschar};

/// Which characters beyond ASCII a URI takes.
#[derive(Clone, Copy)]
pub(super) enum Chars {
    /// None: a URI.
    Ascii,
    /// RFC 3987's `ucschar`, and `iprivate` in the query: an IRI.
    Unicode,
}

/// An ASCII byte's classes, as bits: what each part of a URI takes.
const REG_NAME: u8 = 1;
const USERINFO: u8 = 2;
/// A relative reference's first segment, which takes no `:`.
const SEGMENT_NC: u8 = 4;
const PATH: u8 = 8;
const QUERY: u8 = 16;
const SCHEME: u8 = 32;
/// Not a class of ASCII: the query of an IRI, which takes `iprivate` too.
const PRIVATE: u8 = 64;

const CLASSES: [u8; 256] = {
    let mut table = [0; 256];
    let mut byte: u8 = 0;
    while byte < 128 {
        let unreserved = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        let sub_delim = matches!(
            byte,
            b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
        );
        let mut class = if unreserved || sub_delim {
            REG_NAME | USERINFO | SEGMENT_NC | PATH | QUERY
        } else {
            match byte {
                b':' => USERINFO | PATH | QUERY,
                b'@' => SEGMENT_NC | PATH | QUERY,
                b'/' => PATH | QUERY,
                b'?' => QUERY,
                _ => 0,
            }
        };
        if byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.') {
            class |= SCHEME;
        }
        table[byte as usize] = class;
        byte += 1;
    }
    table
};

/// `scheme:` and the rest: an absolute URI (which may have a fragment, as draft 2020-12 has it).
pub(super) fn absolute(text: &[u8], chars: Chars) -> bool {
    let colon = scheme_end(text);
    text.get(colon) == Some(&b':')
        && text.first().is_some_and(u8::is_ascii_alphabetic)
        && hier(text, colon + 1, chars)
}

/// An absolute URI, or a relative reference, whose first segment takes no `:`.
pub(super) fn reference(text: &[u8], chars: Chars) -> bool {
    if let Some(b'/' | b'?' | b'#') | None = text.first() {
        return hier(text, 0, chars);
    }
    let colon = scheme_end(text);
    if text.get(colon) == Some(&b':') {
        return colon > 0 && absolute(text, chars);
    }
    // The scheme's characters are a segment's too: the first segment goes on from there.
    let end = scan(text, colon, SEGMENT_NC, chars);
    match text.get(end) {
        Some(b':') => false,
        _ if end == 0 => hier(text, 0, chars),
        _ => rest(text, end, chars),
    }
}

/// Where the characters a scheme takes stop.
fn scheme_end(text: &[u8]) -> usize {
    run(text, 0, SCHEME)
}

/// Where, from `at`, the ASCII bytes of `class` stop: a plain loop, which even a build for
/// size keeps tight.
#[inline(always)]
fn run(text: &[u8], mut at: usize, class: u8) -> usize {
    while let Some(&byte) = text.get(at)
        && class_of(byte) & class != 0
    {
        at += 1;
    }
    at
}

/// From `at`: `//` and an authority then a path, or a path alone; then the query and fragment.
fn hier(text: &[u8], at: usize, chars: Chars) -> bool {
    if text.get(at..).is_some_and(|hier| hier.starts_with(b"//")) {
        authority(text, at + 2, chars).is_some_and(|end| rest(text, end, chars))
    } else {
        rest(text, at, chars)
    }
}

/// From `at`: a path, then `?` and a query, then `#` and a fragment, then nothing.
fn rest(text: &[u8], at: usize, chars: Chars) -> bool {
    // A URI's query takes what its path does and `?`: one scan reads both.
    let path = match chars {
        Chars::Ascii => QUERY,
        Chars::Unicode => PATH,
    };
    let mut at = scan(text, at, path, chars);
    if text.get(at) == Some(&b'?') {
        at = scan(text, at + 1, QUERY | PRIVATE, chars);
    }
    if text.get(at) == Some(&b'#') {
        at = scan(text, at + 1, QUERY, chars);
    }
    at == text.len()
}

/// `[userinfo@]host[:port]` from `start`, the host a name or an IP literal in brackets: where it
/// ends, at a `/`, `?`, `#` or the end; `None` when it is not one.
fn authority(text: &[u8], start: usize, chars: Chars) -> Option<usize> {
    let mut host = start;
    let mut end = scan(text, start, REG_NAME, chars);
    match text.get(end) {
        Some(b'@') => host = end + 1,
        Some(b':') => {
            let after = scan(text, end, USERINFO, chars);
            if text.get(after) == Some(&b'@') {
                host = after + 1;
            }
        }
        _ => {}
    }
    if text.get(host) == Some(&b'[') {
        let literal = text.get(host + 1..)?;
        let close = literal.iter().position(|&c| c == b']')?;
        ip_literal(literal.get(..close)?).then_some(())?;
        end = host + close + 2;
    } else if host != start {
        end = scan(text, host, REG_NAME, chars);
    }
    if text.get(end) == Some(&b':') {
        let port = text.get(end + 1..)?;
        end += 1 + port.iter().take_while(|c| c.is_ascii_digit()).count();
    }
    match text.get(end) {
        None | Some(b'/' | b'?' | b'#') => Some(end),
        Some(_) => None,
    }
}

/// An IPv6 address, or `v`, hex digits, `.` and what an `IPvFuture` takes.
fn ip_literal(address: &[u8]) -> bool {
    if let [b'v' | b'V', rest @ ..] = address
        && let Some(dot) = rest.iter().position(|&c| c == b'.')
        && let Some((version, name)) = rest.split_at_checked(dot)
    {
        let name = name.get(1..).unwrap_or_default();
        return !version.is_empty()
            && version.iter().all(u8::is_ascii_hexdigit)
            && !name.is_empty()
            && name.iter().all(|&c| class_of(c) & USERINFO != 0);
    }
    ipv6(address)
}

/// The classes `byte` is of.
#[inline(always)]
fn class_of(byte: u8) -> u8 {
    CLASSES[usize::from(byte)]
}

/// Where, from `at`, the characters of `class` stop: those of the class, a percent-encoded byte,
/// and a code point beyond ASCII that `chars` takes there.
fn scan(text: &[u8], mut at: usize, class: u8, chars: Chars) -> usize {
    loop {
        at = run(text, at, class);
        let Some(&byte) = text.get(at) else {
            return at;
        };
        if byte != b'%' && byte < 0x80 {
            return at;
        }
        let rest = text.get(at..).unwrap_or_default();
        let length = match (byte, chars) {
            (b'%', _) if percent_encoded(rest) => 3,
            (0x80..=0xFF, Chars::Unicode) => match code_point(rest) {
                Some((point, length))
                    if ucschar(point) || class & PRIVATE != 0 && iprivate(point) =>
                {
                    length
                }
                _ => return at,
            },
            _ => return at,
        };
        at += length;
    }
}
