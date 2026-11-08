//! The `format` keyword's checks (draft 2020-12), each strict to its RFC as the official
//! JSON-Schema-Test-Suite reads it: dates and times (`time.rs`), IP addresses (`ip.rs`),
//! hostnames and email addresses (`host.rs`, an A-label's Unicode in `idna.rs`), URIs and IRIs
//! (`uri.rs`), JSON pointers (`pointer.rs`), URI templates (`template.rs`) and UUIDs. Each
//! scans the bytes once, with no allocation and no regular expression: it runs on every answer a
//! model gives. The release is built for size (`opt-level = "z"`), which calls what it could
//! inline: the few helpers every byte goes through are inlined always, and the loops are plain.
//! `regex`, `idn-hostname` and `idn-email` are not judged here: they need a regular expression
//! engine or Unicode's tables.

mod host;
mod idna;
mod ip;
mod marks;
mod pointer;
mod template;
mod time;
mod uri;

use uri::Chars;

/// A format's check, found once by its name when the schema is compiled.
pub(crate) type Check = fn(&[u8]) -> bool;

/// How a hostname is read: as RFC 1123 alone (drafts 4 and 6), or with IDNA's rules for an
/// A-label (from draft 7).
#[derive(Clone, Copy)]
pub(crate) enum Hostnames {
    Rfc1123,
    Idna,
}

/// The check of the format `name`; `None` for a format not known here.
pub(crate) fn resolve(name: &str, hostnames: Hostnames) -> Option<Check> {
    Some(match name {
        "hostname" => match hostnames {
            Hostnames::Rfc1123 => host::rfc1123,
            Hostnames::Idna => host::hostname,
        },
        "date-time" => time::date_time,
        "date" => time::date,
        "time" => time::time,
        "duration" => time::duration,
        "email" => host::email,
        "ipv4" => ip::ipv4,
        "ipv6" => ip::ipv6,
        "uri" => |text| uri::absolute(text, Chars::Ascii),
        "uri-reference" => |text| uri::reference(text, Chars::Ascii),
        "iri" => |text| uri::absolute(text, Chars::Unicode),
        "iri-reference" => |text| uri::reference(text, Chars::Unicode),
        "uuid" => uuid,
        "json-pointer" => pointer::json_pointer,
        "relative-json-pointer" => pointer::relative,
        "regex" => |text| std::str::from_utf8(text).is_ok_and(crate::pattern::is_regex),
        "uri-template" => template::uri_template,
        _ => return None,
    })
}

/// The formats of JSON Schema not checked here: they need Unicode's tables. A schema that asks
/// one is told so; any other name is the schema's own annotation, which asks nothing.
pub(crate) fn unchecked(name: &str) -> bool {
    matches!(name, "idn-hostname" | "idn-email")
}

/// Whether `text` is valid as the format `name`; `None` for a format not known here.
#[cfg(test)]
pub(crate) fn check(name: &str, text: &str) -> Option<bool> {
    resolve(name, Hostnames::Idna).map(|check| check(text.as_bytes()))
}

/// 8-4-4-4-12 hex digits, in any case (RFC 9562).
fn uuid(text: &[u8]) -> bool {
    let Ok(t) = <&[u8; 36]>::try_from(text) else {
        return false;
    };
    // The 32 digits as four words of eight bytes, each checked at once.
    let words = [
        [t[0], t[1], t[2], t[3], t[4], t[5], t[6], t[7]],
        [t[9], t[10], t[11], t[12], t[14], t[15], t[16], t[17]],
        [t[19], t[20], t[21], t[22], t[24], t[25], t[26], t[27]],
        [t[28], t[29], t[30], t[31], t[32], t[33], t[34], t[35]],
    ];
    // Every check made, none skipped: no branch to mispredict.
    let [first, second, third, fourth] = words.map(|word| hex_word(u64::from_le_bytes(word)));
    ([t[8], t[13], t[18], t[23]] == [b'-'; 4]) & first & second & third & fourth
}

/// Whether all eight bytes of `word` are ASCII hex digits: each byte's top bit says whether it
/// is in a range, with no byte's sum carried into the next when none is past 0x7F (a word with
/// one past it is refused whatever the sums say).
#[inline(always)]
fn hex_word(word: u64) -> bool {
    const ONES: u64 = 0x0101_0101_0101_0101;
    const TOPS: u64 = ONES * 0x80;
    let at_least = |x: u64, low: u64| x.wrapping_add((0x80 - low) * ONES);
    let lower = word | (ONES * 0x20);
    let digit = at_least(word, 0x30) & !at_least(word, 0x3A);
    let letter = at_least(lower, 0x61) & !at_least(lower, 0x67);
    word & TOPS == 0 && (digit | letter) & TOPS == TOPS
}

/// The number two ASCII digits write.
#[inline(always)]
fn two_digits(tens: u8, ones: u8) -> Option<u8> {
    let (tens, ones) = (tens.wrapping_sub(b'0'), ones.wrapping_sub(b'0'));
    (tens < 10 && ones < 10).then(|| tens * 10 + ones)
}

/// Whether `text` starts with a percent-encoded byte: `%` and two hex digits.
#[inline(always)]
fn percent_encoded(text: &[u8]) -> bool {
    matches!(text, [b'%', high, low, ..] if high.is_ascii_hexdigit() && low.is_ascii_hexdigit())
}

/// The code point `text` starts with and its length in bytes, `text` being UTF-8.
fn code_point(text: &[u8]) -> Option<(u32, usize)> {
    let (&lead, rest) = text.split_first()?;
    let (length, bits) = match lead {
        0..=0x7F => return Some((u32::from(lead), 1)),
        0x80..=0xDF => (2, lead & 0x1F),
        0xE0..=0xEF => (3, lead & 0x0F),
        0xF0..=0xFF => (4, lead & 0x07),
    };
    let point = rest
        .get(..length - 1)?
        .iter()
        .fold(u32::from(bits), |point, &byte| {
            point << 6 | u32::from(byte & 0x3F)
        });
    Some((point, length))
}

/// RFC 3987's `ucschar`: what an IRI and a URI template's literal take beyond ASCII.
fn ucschar(c: u32) -> bool {
    matches!(c, 0xA0..=0xD7FF | 0xF900..=0xFDCF | 0xFDF0..=0xFFEF)
        || (0x1_0000..=0xE_FFFD).contains(&c)
            && c & 0xFFFF <= 0xFFFD
            && !(0xE_0000..0xE_1000).contains(&c)
}

/// RFC 3987's `iprivate`: the private use code points an IRI takes in its query.
fn iprivate(c: u32) -> bool {
    matches!(c, 0xE000..=0xF8FF | 0xF_0000..=0xF_FFFD | 0x10_0000..=0x10_FFFD)
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
