//! A call the output limit cut off. Streamed, it arrives with what it wrote: JSON arguments that
//! stop midway. What a text tool's call wrote is kept as the text it is (B7).

use serde_json::Value;

/// Parses tried: the text as it came, then with its last escape left out, twice at most (half a
/// pair of surrogates is two escapes).
const TRIES: usize = 3;

/// `raw` with the string it stops in closed, then every object and array still open; `None` when
/// it does not stop inside a string, or is not JSON even then. An escape the cut fell in (`\`,
/// `\u12`, half a surrogate pair) is left out: its character was not written whole.
pub fn closed(raw: &str) -> Option<Value> {
    let mut open = Vec::new();
    let (mut in_string, mut escaped) = (false, false);
    for c in raw.chars() {
        match (in_string, escaped, c) {
            (true, true, _) => escaped = false,
            (true, false, '\\') => escaped = true,
            (true, false, '"') => in_string = false,
            (false, _, '"') => in_string = true,
            (false, _, '{') => open.push('}'),
            (false, _, '[') => open.push(']'),
            (false, _, '}' | ']') => {
                open.pop();
            }
            _ => {}
        }
    }
    if !in_string {
        return None;
    }
    let ends: String = std::iter::once('"')
        .chain(open.iter().rev().copied())
        .collect();
    let mut text = raw;
    for _ in 0..TRIES {
        if let Ok(value) = serde_json::from_str(&format!("{text}{ends}")) {
            return Some(value);
        }
        text = text.get(..text.rfind('\\')?)?;
    }
    None
}

#[cfg(test)]
#[path = "cut_tests.rs"]
mod tests;
