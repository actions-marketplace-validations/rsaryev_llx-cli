//! When a single `$` opens or closes a formula: pandoc's rule, which GitHub follows, and a dollar
//! sign by its own look (a price, a shell variable) never does. So `$x$` is a formula, and
//! `$ x $`, `$a$5`, `costs $5 and $10`, `$HOME/bin and $PATH` and a shell line's
//! `$(seq 1 12) ... $i` are text.

use crate::scan::is_blank;

/// How far after a `$` its look is read: a price or a name is shorter.
const WINDOW: usize = 48;

/// Whether the single `$` at `at` can open a formula: something not blank follows it.
pub(super) fn opens(text: &[u8], at: usize) -> bool {
    text.get(at + 1).is_some_and(|byte| !is_blank(*byte)) && !literal(text, at)
}

/// Whether the single `$` at `at` can close one: something not blank before it, no digit after
/// it, no backslash that escapes it.
pub(super) fn closes(text: &[u8], at: usize) -> bool {
    let before = at.checked_sub(1).and_then(|before| text.get(before));
    if before.is_none_or(|byte| is_blank(*byte)) || text.get(at + 1).is_some_and(u8::is_ascii_digit)
    {
        return false;
    }
    let slashes = text[..at].iter().rev().take_while(|b| **b == b'\\').count();
    slashes % 2 == 0 && !literal(text, at)
}

/// Whether the `$` at `at` is a dollar sign by what follows it: an amount (`$5`, `$1.5M`,
/// `$50/month`) or a variable (`$HOME/bin`, `$PATH,`, `$PATH before`).
fn literal(text: &[u8], at: usize) -> bool {
    let after = text.get(at + 1..).unwrap_or_default();
    let after = after.get(..WINDOW).unwrap_or(after);
    amount(after) || variable(after)
}

/// Digits, with points and commas among them and a magnitude after, then what ends an amount.
fn amount(after: &[u8]) -> bool {
    if !after.first().is_some_and(u8::is_ascii_digit) {
        return false;
    }
    let run = after
        .iter()
        .take_while(|b| b.is_ascii_digit() || matches!(b, b'.' | b','))
        .count();
    // A point or a comma inside the run may itself end the amount: `$10,`, `$1.5M`.
    if after[1..run].iter().any(|b| matches!(b, b'.' | b',')) {
        return true;
    }
    let rest = &after[run..];
    let magnitude = rest.first().is_some_and(|b| b"kKmMbB".contains(b));
    ends_amount(rest) || (magnitude && ends_amount(&rest[1..]))
}

/// What may follow an amount: a blank, punctuation, a dash, a slash, or nothing.
fn ends_amount(rest: &[u8]) -> bool {
    match rest {
        [] => true,
        [0xe2, 0x80, 0x93 | 0x94, ..] => true,
        [byte, ..] => is_blank(*byte) || b".,;:!?)]'\"`/-".contains(byte),
    }
}

/// An upper-case name of two characters or more, then what ends a path or a list; or of three
/// or more, then a blank.
fn variable(after: &[u8]) -> bool {
    if !after
        .first()
        .is_some_and(|b| b.is_ascii_uppercase() || *b == b'_')
    {
        return false;
    }
    let name = after
        .iter()
        .take_while(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || **b == b'_')
        .count();
    let rest = &after[name..];
    match rest.first() {
        None => name >= 2,
        Some(byte) if b"/:.,;)]'\"`".contains(byte) => name >= 2,
        Some(byte) => name >= 3 && is_blank(*byte),
    }
}
