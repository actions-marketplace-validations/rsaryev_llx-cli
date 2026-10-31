//! Text from the model or a tool, made safe to show at a terminal. A carriage return, an escape
//! sequence or a mark that reorders text could redraw what the person reads, so they would
//! approve one command and run another; each is shown as an escape instead (`\r`, `\u{1b}`).

/// `text` with every control character and every reordering mark escaped.
pub(crate) fn printable(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_control() || reorders(c) {
            out.extend(c.escape_default());
        } else {
            out.push(c);
        }
    }
    out
}

/// The bidirectional marks and overrides that change the order text is shown in.
fn reorders(c: char) -> bool {
    matches!(
        c,
        '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
    )
}

#[cfg(test)]
#[path = "printable_tests.rs"]
mod tests;
