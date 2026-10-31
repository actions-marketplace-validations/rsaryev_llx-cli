//! Wildcards in one place: a pattern matched against a text with one backtrack point, so a
//! failing match costs at most the pattern times the text, whatever the model's text is.

/// What a pattern's wildcards stand for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Wild {
    /// `*` is any text; every other byte is itself.
    Star,
    /// `*` is any text and `?` any one byte.
    StarAndOne,
}

/// Whether `pattern` matches the whole `text`.
pub(super) fn matches(pattern: &[u8], text: &[u8], wild: Wild) -> bool {
    let (mut at, mut seen) = (0, 0);
    // Where the last `*` left off: the pattern after it, and the text it has taken so far.
    let mut star = None;
    loop {
        match (pattern.get(at), text.get(seen)) {
            (Some(b'*'), _) => {
                at += 1;
                star = Some((at, seen));
                continue;
            }
            (None, None) => return true,
            (Some(&want), Some(&got))
                if want == got || (want == b'?' && wild == Wild::StarAndOne) =>
            {
                at += 1;
                seen += 1;
                continue;
            }
            _ => {}
        }
        // A mismatch: the last `*` takes one byte more, if there is one left.
        match star {
            Some((after, taken)) if taken < text.len() => {
                (at, seen) = (after, taken + 1);
                star = Some((after, seen));
            }
            _ => return false,
        }
    }
}
