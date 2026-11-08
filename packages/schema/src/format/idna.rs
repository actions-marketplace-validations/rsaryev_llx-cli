//! A hostname's A-label (`xn--`, RFC 5890): its Punycode (RFC 3492) decoded into a buffer on
//! the stack, then the U-label held to RFC 5891 and the contextual rules and exceptions of RFC
//! 5892 that need no table but the marks' (`marks.rs`). A code point's general category is not
//! looked up: the blocks that hold no letter, mark or digit are refused whole (`never_valid`),
//! but a symbol or punctuation elsewhere is taken, where a full IDNA check refuses it.

use super::marks::{is_mark, is_virama};

/// The most code points a label decodes to: a label is 63 bytes at most.
const MOST: usize = 63;

/// Whether `payload`, what follows `xn--`, is the Punycode of a valid U-label.
pub(super) fn a_label(payload: &[u8]) -> bool {
    let mut decoded = [0; MOST];
    decode(payload, &mut decoded)
        .and_then(|length| decoded.get(..length))
        .is_some_and(u_label)
}

/// RFC 3492, 6.2: `payload` decoded into `out`, and how many code points it holds.
fn decode(payload: &[u8], out: &mut [u32; MOST]) -> Option<usize> {
    let (basic, encoded) = match payload.iter().rposition(|&c| c == b'-') {
        Some(at) if at > 0 => (payload.get(..at)?, payload.get(at + 1..)?),
        _ => (&[][..], payload),
    };
    let mut length = 0;
    for &c in basic {
        *out.get_mut(length)? = u32::from(c);
        length += 1;
    }
    let (mut point, mut index, mut bias) = (128_u32, 0_u32, 72_u32);
    let mut digits = encoded.iter();
    while digits.len() > 0 {
        let before = index;
        let mut weight = 1_u32;
        let mut k = 36_u32;
        loop {
            let digit = digit(*digits.next()?)?;
            index = index.checked_add(digit.checked_mul(weight)?)?;
            let threshold = k.saturating_sub(bias).clamp(1, 26);
            if digit < threshold {
                break;
            }
            weight = weight.checked_mul(36 - threshold)?;
            k += 36;
        }
        let count = u32::try_from(length).ok()? + 1;
        bias = adapt(index - before, count, before == 0);
        point = point.checked_add(index / count)?;
        index %= count;
        // A basic code point is never encoded, and a surrogate is no character.
        if point < 0x80 || char::from_u32(point).is_none() {
            return None;
        }
        let at = usize::try_from(index).ok()?;
        let moved = out.get_mut(at..=length)?;
        moved.rotate_right(1);
        *moved.first_mut()? = point;
        length += 1;
        index += 1;
    }
    Some(length)
}

/// A Punycode digit's value.
fn digit(c: u8) -> Option<u32> {
    match c {
        b'a'..=b'z' => Some(u32::from(c - b'a')),
        b'A'..=b'Z' => Some(u32::from(c - b'A')),
        b'0'..=b'9' => Some(u32::from(c - b'0') + 26),
        _ => None,
    }
}

/// RFC 3492, 6.1: the bias after a code point is decoded.
fn adapt(delta: u32, count: u32, first: bool) -> u32 {
    let mut delta = if first { delta / 700 } else { delta / 2 };
    delta += delta / count;
    let mut k = 0;
    while delta > 35 * 26 / 2 {
        delta /= 35;
        k += 36;
    }
    k + 36 * delta / (delta + 38)
}

/// Whether a decoded label is a U-label: some code point beyond ASCII (its ASCII the hostname's
/// letters, digits and hyphens, in either case), no mark first, no hyphen first, last or third and
/// fourth, and RFC 5892's exceptions and contexts.
fn u_label(label: &[u32]) -> bool {
    let (Some(&first), Some(&last)) = (label.first(), label.last()) else {
        return false;
    };
    let hyphen = u32::from(b'-');
    if is_mark(first)
        || first == hyphen
        || last == hyphen
        || label.get(2..4) == Some(&[hyphen, hyphen][..])
        || label.iter().all(|&c| c < 0x80)
    {
        return false;
    }
    let mut kana_or_han = false;
    let mut katakana_dot = false;
    let mut arabic_digits = false;
    let mut extended_digits = false;
    for (at, &c) in label.iter().enumerate() {
        let before = at.checked_sub(1).and_then(|at| label.get(at)).copied();
        let after = label.get(at + 1).copied();
        let allowed = match c {
            // A.1: a ZERO WIDTH NON-JOINER, taken anywhere: its context is a virama or the
            // joining types of Arabic letters around it, which need a table.
            0x200C => true,
            // A.2: a ZERO WIDTH JOINER after a virama.
            0x200D => before.is_some_and(is_virama),
            // A.3: a MIDDLE DOT between two `l`s.
            0xB7 => before == Some(0x6C) && after == Some(0x6C),
            // A.4: a Greek KERAIA before Greek.
            0x375 => after.is_some_and(|c| matches!(c, 0x370..=0x3FF | 0x1F00..=0x1FFF)),
            // A.5, A.6: a Hebrew GERESH or GERSHAYIM after Hebrew.
            0x5F3 | 0x5F4 => before.is_some_and(|c| matches!(c, 0x590..=0x5FF | 0xFB1D..=0xFB4F)),
            0x30FB => {
                katakana_dot = true;
                true
            }
            0x660..=0x669 => {
                arabic_digits = true;
                true
            }
            0x6F0..=0x6F9 => {
                extended_digits = true;
                true
            }
            // 2.6: the exceptions DISALLOWED.
            0x640 | 0x7FA | 0x302E | 0x302F | 0x3031..=0x3035 | 0x303B => false,
            _ => !never_valid(c),
        };
        if !allowed {
            return false;
        }
        kana_or_han |= kana_or_han_script(c);
    }
    // A.7: a KATAKANA MIDDLE DOT with Hiragana, Katakana or Han; A.8, A.9: one kind of Arabic
    // digits.
    (kana_or_han || !katakana_dot) && !(arabic_digits && extended_digits)
}

/// Whether `c` is of the Hiragana, Katakana or Han script, but the KATAKANA MIDDLE DOT.
fn kana_or_han_script(c: u32) -> bool {
    c != 0x30FB
        && matches!(
            c,
            0x2E80..=0x2FDF
                | 0x3005
                | 0x3007
                | 0x3021..=0x3029
                | 0x3038..=0x303B
                | 0x3041..=0x30FF
                | 0x31F0..=0x31FF
                | 0x3400..=0x4DBF
                | 0x4E00..=0x9FFF
                | 0xF900..=0xFAFF
                | 0x1_B000..=0x1_B16F
                | 0x2_0000..=0x3_FFFF
        )
}

/// Whether `c` is in a block that holds no letter, mark or digit a label may take: controls,
/// Latin-1's signs (with the ordinal and micro signs, which IDNA2008 refuses too), punctuation,
/// symbols, emoji, surrogates, private use, tags, the planes nothing is assigned in yet and the
/// noncharacters.
fn never_valid(c: u32) -> bool {
    c & 0xFFFE == 0xFFFE
        || matches!(
            c,
            0x80..=0xBF
                | 0xD7
                | 0xF7
                | 0x2000..=0x2070
                | 0x20A0..=0x20CF
                | 0x2185..=0x2BFF
                | 0x2E00..=0x2E2E
                | 0x2E30..=0x3004
                | 0x3200..=0x33FF
                | 0xD800..=0xF8FF
                | 0xFDD0..=0xFDEF
                | 0xFFF0..=0xFFFF
                | 0x1_EEBC..=0x1_FBEF
                | 0x1_FBFA..=0x1_FFFF
                | 0x3_23B0..=0xE_00FF
                | 0xE_01F0..=0x10_FFFF
        )
}
