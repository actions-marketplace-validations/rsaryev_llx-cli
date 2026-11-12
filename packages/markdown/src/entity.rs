//! Character references (`&amp;`, `&#35;`, `&#x22;`) and backslash escapes, decoded.

use crate::scan::is_ascii_punct;

/// Every named reference, a line each, sorted by name: the name, then its code points in hex
/// (`data/make.py`). Searched as it is, so nothing is built before the first lookup.
const TABLE: &[u8] = include_bytes!("../data/entities.txt");

/// Where the names that start with each ASCII byte start in the table, and where the last end.
const fn starts() -> [usize; 129] {
    let mut starts = [TABLE.len(); 129];
    let mut line = TABLE.len();
    // From the last line back, so each byte is left with its first line.
    let mut at = TABLE.len();
    while at > 0 {
        at -= 1;
        if at == 0 || TABLE[at - 1] == b'\n' {
            if at < line && TABLE[at] < 128 {
                let mut byte = TABLE[at] as usize;
                // Every byte up to this one with no name of its own starts here too.
                loop {
                    starts[byte] = at;
                    if byte == 0 || starts[byte - 1] < at {
                        break;
                    }
                    byte -= 1;
                }
            }
            line = at;
        }
    }
    starts
}

const STARTS: [usize; 129] = starts();

/// The code points `name` stands for, as the table writes them.
fn named(name: &[u8]) -> Option<&'static [u8]> {
    let first = usize::from(*name.first()?);
    let (mut low, mut high) = (*STARTS.get(first)?, *STARTS.get(first + 1)?);
    while low < high {
        let mid = usize::midpoint(low, high);
        let start = TABLE[..mid]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |at| at + 1);
        let end = TABLE[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(TABLE.len(), |at| at + start);
        let line = &TABLE[start..end];
        let space = line.iter().position(|byte| *byte == b' ')?;
        match line[..space].cmp(name) {
            std::cmp::Ordering::Less => low = end + 1,
            std::cmp::Ordering::Greater => high = start,
            std::cmp::Ordering::Equal => return Some(&line[space + 1..]),
        }
    }
    None
}

fn push_point(code: u32, out: &mut String) {
    out.push(
        char::from_u32(code)
            .filter(|ch| *ch != '\0')
            .unwrap_or('\u{fffd}'),
    );
}

/// A number's digits from `at`, in `radix`, at most `most` of them: its value and where it ends.
fn number(text: &[u8], at: usize, radix: u32, most: usize) -> Option<(u32, usize)> {
    let mut value = 0u32;
    let mut end = at;
    while let Some(digit) = text.get(end).and_then(|b| char::from(*b).to_digit(radix)) {
        value = value * radix + digit;
        end += 1;
        if end - at > most {
            return None;
        }
    }
    (end > at && text.get(end) == Some(&b';')).then_some((value, end + 1))
}

/// The reference that starts at `text[at]` (an `&`), decoded onto `out`: where it ends.
pub(crate) fn entity(text: &[u8], at: usize, out: &mut String) -> Option<usize> {
    if text.get(at + 1) == Some(&b'#') {
        let hex = matches!(text.get(at + 2), Some(b'x' | b'X'));
        let (code, end) = if hex {
            number(text, at + 3, 16, 6)?
        } else {
            number(text, at + 2, 10, 7)?
        };
        push_point(code, out);
        return Some(end);
    }
    let mut end = at + 1;
    while text.get(end).is_some_and(u8::is_ascii_alphanumeric) {
        end += 1;
    }
    if end == at + 1 || text.get(end) != Some(&b';') {
        return None;
    }
    let name = &text[at + 1..end];
    // The few a model writes, known without the table.
    let common = match name {
        b"amp" => '&',
        b"lt" => '<',
        b"gt" => '>',
        b"quot" => '"',
        b"nbsp" => '\u{a0}',
        _ => '\0',
    };
    if common != '\0' {
        out.push(common);
        return Some(end + 1);
    }
    let kept = out.len();
    for point in named(name)?.split(|byte| *byte == b' ') {
        let code = str::from_utf8(point)
            .ok()
            .and_then(|hex| u32::from_str_radix(hex, 16).ok());
        let Some(code) = code else {
            out.truncate(kept);
            return None;
        };
        push_point(code, out);
    }
    Some(end + 1)
}

/// Whether `text` holds anything `unescape` would change.
pub(crate) fn escaped(text: &[u8]) -> bool {
    text.iter().any(|byte| matches!(byte, b'\\' | b'&'))
}

/// `text` with its backslash escapes and its references decoded, onto `out`.
pub(crate) fn unescape(text: &str, out: &mut String) {
    let bytes = text.as_bytes();
    let mut from = 0;
    let mut at = 0;
    while at < bytes.len() {
        let next = match bytes[at] {
            b'\\' if bytes.get(at + 1).is_some_and(|b| is_ascii_punct(*b)) => {
                out.push_str(text.get(from..at).unwrap_or_default());
                from = at + 1;
                at + 2
            }
            b'&' => {
                // What is before the reference first: a reference that is none writes nothing.
                let kept = out.len();
                out.push_str(text.get(from..at).unwrap_or_default());
                if let Some(end) = entity(bytes, at, out) {
                    from = end;
                    end
                } else {
                    out.truncate(kept);
                    at + 1
                }
            }
            _ => at + 1,
        };
        at = next;
    }
    out.push_str(text.get(from..).unwrap_or_default());
}

#[cfg(test)]
#[path = "entity_tests.rs"]
mod tests;
