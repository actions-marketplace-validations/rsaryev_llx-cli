//! IP addresses: IPv4's dotted quad (RFC 2673's `dotted-quad`) and IPv6's text form (RFC 4291,
//! section 2.2), with no zone and no prefix.

/// Four decimal octets, 0 to 255, none with a leading zero.
pub(super) fn ipv4(text: &[u8]) -> bool {
    let Some(mut rest) = octet(text) else {
        return false;
    };
    for _ in 0..3 {
        rest = match rest.split_first() {
            Some((b'.', after)) => match octet(after) {
                Some(after) => after,
                None => return false,
            },
            _ => return false,
        };
    }
    rest.is_empty()
}

/// What follows the octet `text` starts with: its longest run of digits, 255 at most, no `0`
/// before another digit (which the caller then finds where a `.` or the end must be).
#[inline(always)]
fn octet(text: &[u8]) -> Option<&[u8]> {
    match *text {
        [b'0', ref rest @ ..] => Some(rest),
        [
            hundreds @ b'1'..=b'9',
            tens @ b'0'..=b'9',
            ones @ b'0'..=b'9',
            ref rest @ ..,
        ] => ((hundreds, tens, ones) <= (b'2', b'5', b'5')).then_some(rest),
        [b'1'..=b'9', b'0'..=b'9', ref rest @ ..] | [b'1'..=b'9', ref rest @ ..] => Some(rest),
        _ => None,
    }
}

/// Eight groups of one to four hex digits, the last two of which may be an IPv4 address, and
/// one `::` at most in place of one group of zeros or more: read in one pass.
pub(super) fn ipv6(text: &[u8]) -> bool {
    let (mut at, mut compressed) = match text {
        [b':', b':', ..] => (2, true),
        _ => (0, false),
    };
    let mut groups = 0;
    while at < text.len() && groups < 8 {
        let rest = text.get(at..).unwrap_or_default();
        let digits = rest
            .iter()
            .take(5)
            .take_while(|c| c.is_ascii_hexdigit())
            .count();
        if rest.get(digits) == Some(&b'.') {
            // An IPv4 address ends the text, as two groups.
            return ipv4(rest) && if compressed { groups <= 5 } else { groups == 6 };
        }
        if digits == 0 || digits > 4 {
            return false;
        }
        groups += 1;
        at += digits;
        match text.get(at..).unwrap_or_default() {
            [] => break,
            [b':', b':', ..] if !compressed => {
                compressed = true;
                at += 2;
            }
            [b':', _, ..] if text.get(at + 1) != Some(&b':') => at += 1,
            _ => return false,
        }
    }
    at == text.len() && if compressed { groups <= 7 } else { groups == 8 }
}
