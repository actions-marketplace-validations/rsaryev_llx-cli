//! JSON pointers (RFC 6901) and relative JSON pointers (draft-bhutton-relative-json-pointer-00).

/// Empty, or `/` and reference tokens, each `~` in them followed by `0` or `1`.
pub(super) fn json_pointer(text: &[u8]) -> bool {
    match text.split_first() {
        None => true,
        Some((b'/', tokens)) => escapes_ok(tokens),
        Some(_) => false,
    }
}

/// A number of levels up (no leading zero), then `#` or a JSON pointer.
pub(super) fn relative(text: &[u8]) -> bool {
    let digits = text.iter().take_while(|c| c.is_ascii_digit()).count();
    let (levels, rest) = text.split_at_checked(digits).unwrap_or_default();
    let number = matches!(levels, [b'0'] | [b'1'..=b'9', ..]);
    number && (rest == b"#" || json_pointer(rest))
}

/// Whether every `~` is followed by `0` or `1`.
fn escapes_ok(tokens: &[u8]) -> bool {
    let mut rest = tokens;
    while let Some(at) = rest.iter().position(|&c| c == b'~') {
        match rest.get(at + 1..) {
            Some([b'0' | b'1', after @ ..]) => rest = after,
            _ => return false,
        }
    }
    true
}
