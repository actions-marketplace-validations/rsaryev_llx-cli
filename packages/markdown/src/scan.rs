//! What the parser asks of a byte or a character: its class, as CommonMark names them.

use crate::tree::at;

/// Unicode's punctuation past ASCII (general categories P and S), as sorted ranges: first and
/// last code point, three bytes each (`data/make.py`).
const PUNCT: &[u8] = include_bytes!("../data/punct.bin");

pub(crate) const fn is_space(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// A space, a tab or a line ending.
pub(crate) const fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

pub(crate) const fn is_ascii_punct(byte: u8) -> bool {
    byte.is_ascii_punctuation()
}

fn point(at: usize) -> u32 {
    PUNCT.get(at..at + 3).map_or(u32::MAX, |b| {
        u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2])
    })
}

pub(crate) fn is_punct(ch: char) -> bool {
    if ch.is_ascii() {
        return ch.is_ascii_punctuation();
    }
    let code = u32::from(ch);
    let (mut low, mut high) = (0, PUNCT.len() / 6);
    while low < high {
        let mid = usize::midpoint(low, high);
        if code < point(mid * 6) {
            high = mid;
        } else if code > point(mid * 6 + 3) {
            low = mid + 1;
        } else {
            return true;
        }
    }
    false
}

/// Unicode whitespace, as CommonMark has it: a space separator, a tab or a line ending.
pub(crate) fn is_whitespace(ch: char) -> bool {
    matches!(ch, '\t' | '\n' | '\x0c' | '\r') || (ch.is_whitespace() && !ch.is_control())
}

/// The character that ends right before `at`.
pub(crate) fn char_before(text: &[u8], at: usize) -> Option<char> {
    let mut start = at.checked_sub(1)?;
    while start > 0 && text[start] & 0xc0 == 0x80 {
        start -= 1;
    }
    str::from_utf8(text.get(start..at)?).ok()?.chars().next()
}

/// The character that starts at `at`.
pub(crate) fn char_at(text: &[u8], at: usize) -> Option<char> {
    let first = *text.get(at)?;
    if first < 0x80 {
        return Some(char::from(first));
    }
    let len = match first {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    };
    str::from_utf8(text.get(at..(at + len).min(text.len()))?)
        .ok()?
        .chars()
        .next()
}

/// `text` without the spaces, tabs and line endings at its two ends: where it starts and ends.
pub(crate) fn trimmed(text: &[u8], mut start: usize, mut end: usize) -> (usize, usize) {
    while start < end && is_blank(text[start]) {
        start += 1;
    }
    while end > start && is_blank(text[end - 1]) {
        end -= 1;
    }
    (start, end)
}

const ONES: u64 = 0x0101_0101_0101_0101;
const TOPS: u64 = ONES << 7;

/// The first byte of `text`, from `from` on, that `is` what is looked for: 32 bytes at a look.
/// `marks` sets the top bit of every byte of a word that may be one, and of none before the
/// first that is; `is` then says which are.
#[inline(always)]
fn first(
    text: &[u8],
    from: usize,
    marks: impl Fn(u64) -> u64,
    is: impl Fn(u8) -> bool,
) -> Option<usize> {
    let rest = text.get(from..)?;
    let marked = |word: &[u8; 8]| marks(u64::from_le_bytes(*word));
    let among = |mut marks: u64, base: usize| {
        while marks != 0 {
            let here = base + at(marks.trailing_zeros() / 8);
            if is(text[here]) {
                return Some(here);
            }
            marks &= marks - 1;
        }
        None
    };
    let (blocks, tail) = rest.as_chunks::<32>();
    let mut done = from;
    for block in blocks {
        let (words, _) = block.as_chunks::<8>();
        let marks = [
            marked(&words[0]),
            marked(&words[1]),
            marked(&words[2]),
            marked(&words[3]),
        ];
        if marks[0] | marks[1] | marks[2] | marks[3] != 0 {
            for (index, marks) in marks.into_iter().enumerate() {
                if let Some(found) = among(marks, done + index * 8) {
                    return Some(found);
                }
            }
        }
        done += 32;
    }
    let (words, tail) = tail.as_chunks::<8>();
    for word in words {
        if let Some(found) = among(marked(word), done) {
            return Some(found);
        }
        done += 8;
    }
    Some(done + tail.iter().position(|byte| is(*byte))?)
}

/// Where `byte` is in `text`, from `from` on.
pub(crate) fn find(text: &[u8], from: usize, byte: u8) -> Option<usize> {
    let wanted = ONES * u64::from(byte);
    // A byte of `differs` is zero where the text has `byte`; the trick marks a zero byte.
    let marks = |word: u64| {
        let differs = word ^ wanted;
        differs.wrapping_sub(ONES) & !differs & TOPS
    };
    first(text, from, marks, |other| other == byte)
}

/// Where the line that holds `text[from]` ends: at its `\n` or `\r`, in one pass for the two.
pub(crate) fn line_end(text: &[u8], from: usize) -> Option<usize> {
    // Marks every byte below 0x0e: the two line endings, and the tab and its like beside them.
    let marks = |word: u64| word.wrapping_sub(ONES * 0x0e) & !word & TOPS;
    first(text, from, marks, |byte| matches!(byte, b'\n' | b'\r'))
}

/// Where that line ends, and where `byte` first is on it: the two in one pass, for a line that
/// would be looked through for `byte` anyway.
pub(crate) fn line_end_and(text: &[u8], from: usize, byte: u8) -> (Option<usize>, Option<usize>) {
    let wanted = ONES * u64::from(byte);
    let marks = |word: u64| {
        let differs = word ^ wanted;
        (word.wrapping_sub(ONES * 0x0e) & !word | differs.wrapping_sub(ONES) & !differs) & TOPS
    };
    let is = |other: u8| matches!(other, b'\n' | b'\r') || other == byte;
    match first(text, from, marks, is) {
        Some(at) if text[at] == byte => (line_end(text, at + 1), Some(at)),
        end => (end, None),
    }
}

/// Where the run of bytes from `from` that are neither blank nor a `<` ends: a bare address.
pub(crate) fn unbroken(text: &[u8], from: usize) -> usize {
    const OPEN: u64 = ONES * 0x3c;
    // Marks every byte below 0x21, the blanks among them, and every `<`.
    let marks = |word: u64| {
        let differs = word ^ OPEN;
        (word.wrapping_sub(ONES * 0x21) & !word | differs.wrapping_sub(ONES) & !differs) & TOPS
    };
    first(text, from, marks, |byte| is_blank(byte) || byte == b'<').unwrap_or(text.len())
}

/// The bytes the inline pass stops at: those that can mean something there.
const fn stops(gfm: bool) -> [bool; 256] {
    let mut table = [false; 256];
    let marks = b"\n\\`*_[]!<&";
    let mut at = 0;
    while at < marks.len() {
        table[marks[at] as usize] = true;
        at += 1;
    }
    if gfm {
        table[b'~' as usize] = true;
        table[b':' as usize] = true;
        table[b'w' as usize] = true;
    }
    table
}

const PLAIN: [bool; 256] = stops(false);
const GITHUB: [bool; 256] = stops(true);

/// How many bytes are looked at one by one before a run of plain text is taken to be long: where
/// the text is full of marks the next one is this near.
const NEAR: usize = 8;

/// Where the inline pass stops next, from `from` on: at a byte that can mean something. With
/// `gfm` at what may start a bare address too, a `:` and a `w`.
pub(crate) fn stop(text: &[u8], from: usize, gfm: bool) -> Option<usize> {
    let table = if gfm { &GITHUB } else { &PLAIN };
    let rest = text.get(from..)?;
    let stops = |byte: &u8| table[usize::from(*byte)];
    let (near, far) = rest.split_at(rest.len().min(NEAR));
    if let Some(at) = near.iter().position(stops) {
        return Some(from + at);
    }
    // A long run of plain text: sixteen bytes compared with every stop at once, with no branch
    // among them, which the compiler turns into a few comparisons of the whole sixteen.
    let may = |byte: u8| {
        let plain = (byte < 0x0e)
            | (byte.wrapping_sub(0x5b) < 6)
            | (byte == b'!')
            | (byte == b'&')
            | (byte == b'*')
            | (byte == b'<');
        plain | (gfm & ((byte == b':') | (byte == b'w') | (byte == b'~')))
    };
    let (chunks, _) = far.as_chunks::<16>();
    let clear = chunks
        .iter()
        .take_while(|chunk| !chunk.iter().fold(false, |any, byte| any | may(*byte)))
        .count();
    let done = near.len() + clear * 16;
    Some(from + done + rest[done..].iter().position(stops)?)
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
