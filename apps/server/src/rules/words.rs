//! A command's words as the shell gives them to the program: quotes and backslashes taken out
//! (`--receive'-pack'=x` is `--receive-pack=x`, `'sh'` is `sh`), a quoted text one word. What a
//! rule must see is what runs, not how the model spelled it.

/// The first word of `text`, unquoted, and the text after it; `None` when there is none.
pub(super) fn first(text: &str) -> Option<(String, &str)> {
    let text = text.trim_start();
    if text.is_empty() {
        return None;
    }
    let mut word = String::new();
    let mut quote = None;
    let mut chars = text.char_indices();
    while let Some((at, c)) = chars.next() {
        match (quote, c) {
            (None, c) if c.is_whitespace() => return Some((word, text.get(at..).unwrap_or(""))),
            (None, '\'' | '"') => quote = Some(c),
            (Some(open), c) if c == open => quote = None,
            (None | Some('"'), '\\') => word.extend(chars.next().map(|(_, c)| c)),
            _ => word.push(c),
        }
    }
    Some((word, ""))
}

/// Every word of `text`, unquoted.
pub(super) fn words(text: &str) -> Vec<String> {
    let mut words = vec![];
    let mut rest = text;
    while let Some((word, after)) = first(rest) {
        words.push(word);
        rest = after;
    }
    words
}

/// A program by its name, wherever it is: `/bin/sh` is `sh`.
pub(super) fn name(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}
