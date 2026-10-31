//! A command's words as the shell gives them to the program: quotes and backslashes taken out
//! (`--receive'-pack'=x` is `--receive-pack=x`, `'sh'` is `sh`), a quoted text one word. What a
//! rule must see is what runs, not how the model spelled it.

/// The first word of `text`, unquoted, and the text after it; `None` when there is none.
pub(super) fn first(text: &str) -> Option<(String, &str)> {
    scan(text).map(|(word, _, after)| (word, after))
}

/// The first word of `text`, unquoted, where in it its first unquoted glob starts (`*`, `?`, or a
/// `[` closed by a `]`: `[ -f x ]` is no glob), and the text after it.
fn scan(text: &str) -> Option<(String, Option<usize>, &str)> {
    let text = text.trim_start();
    if text.is_empty() {
        return None;
    }
    let (mut word, mut quote, mut glob, mut bracket) = (String::new(), None, None, None);
    let earliest = |glob: Option<usize>, at: usize| Some(glob.map_or(at, |glob| at.min(glob)));
    let mut chars = text.char_indices();
    while let Some((at, c)) = chars.next() {
        match (quote, c) {
            (None, c) if c.is_whitespace() => {
                return Some((word, glob, text.get(at..).unwrap_or("")));
            }
            (None, '\'' | '"') => quote = Some(c),
            (Some(open), c) if c == open => quote = None,
            // A backslash before a line break joins the lines: the shell drops both.
            (None | Some('"'), '\\') => {
                word.extend(chars.next().map(|(_, c)| c).filter(|c| *c != '\n'));
            }
            (None, '*' | '?' | '[' | ']') => {
                match c {
                    '[' => bracket = bracket.or(Some(word.len())),
                    ']' => glob = bracket.map_or(glob, |open| earliest(glob, open)),
                    _ => glob = earliest(glob, word.len()),
                }
                word.push(c);
            }
            _ => word.push(c),
        }
    }
    Some((word, glob, ""))
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

/// The words of a command that may be options, up to the `--` that ends them. A `--` right after
/// an option is read as that option's value, as git and ripgrep read it (`rg -e -- --pre sh`
/// searches for `--` and still runs `sh`), so the options go on past it.
pub(super) fn options(words: &[String]) -> &[String] {
    let value = |before: &String| before.starts_with('-') && !before.contains('=');
    let end = (words.windows(2))
        .position(|pair| matches!(pair, [before, word] if word == "--" && !value(before)));
    end.and_then(|end| words.get(..=end)).unwrap_or(words)
}

/// For each word of `text`, whether the shell could put a file name there that reads as an option:
/// an unquoted glob with nothing before it, or only an option's name (`*`, `-*`, `--p*`). One after
/// a value or a path (`--glob=*.rs`, `src/*`, `./*`) gives names that start with it.
pub(super) fn spilled(text: &str) -> Vec<bool> {
    let mut spilled = vec![];
    let mut rest = text;
    while let Some((word, glob, after)) = scan(rest) {
        let head = glob.and_then(|glob| word.get(..glob));
        spilled.push(
            head.is_some_and(|head| {
                head.is_empty() || (head.starts_with('-') && !head.contains('='))
            }),
        );
        rest = after;
    }
    spilled
}

/// Whether the first word of `text` has an unquoted glob: the shell, not the line, picks the
/// program (`b*` may be `bash`).
pub(super) fn glob_first(text: &str) -> bool {
    scan(text).is_some_and(|(_, glob, _)| glob.is_some())
}

/// A program by its name, wherever it is: `/bin/sh` is `sh`.
pub(super) fn name(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}
