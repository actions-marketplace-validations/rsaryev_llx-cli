//! A search's words: the terms a question names, and the words of a text split as code names
//! things (`parse_rows`, `parseRows` and `ParseRows` are each `parse` and `rows` too), so a term
//! finds a name in any case and in its inflections (`kills` finds `kill` and `killpg`).

use super::pattern::Pattern;

/// One word of the question, as it is looked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Term {
    /// The word as asked, lower-cased: what is shown.
    pub(super) word: String,
    /// The word without its ending: a text's word is this term when it is this with an
    /// inflection's ending (`renames` finds `rename` and `renamed`, not `renamer`), or starts with
    /// the whole word (`kill` finds `killpg`).
    pub(super) stem: String,
    /// A short word is matched whole (or with a plural `s`), never as a prefix: `id` is not
    /// `idle`.
    pub(super) exact: bool,
    /// A number is found in any of its spellings: `0600` is `0o600` and `600`.
    pub(super) number: bool,
    /// `-e`'s patterns: a line, not a word, matches it.
    pub(super) pattern: Option<Pattern>,
}

impl Term {
    /// `-e`'s patterns as a term.
    pub(super) fn pattern(pattern: Pattern) -> Self {
        Self {
            word: String::new(),
            stem: String::new(),
            exact: true,
            number: false,
            pattern: Some(pattern),
        }
    }

    /// Whether `word` (lower-cased) is this term.
    pub(super) fn finds(&self, word: &str) -> bool {
        if self.pattern.is_some() {
            return false;
        }
        if self.number {
            return word.starts_with(|c: char| c.is_ascii_digit()) && number(word) == self.stem;
        }
        if self.exact {
            return word == self.stem || word.strip_suffix('s') == Some(self.stem.as_str());
        }
        word.starts_with(self.word.as_str())
            || word
                .strip_prefix(self.stem.as_str())
                .is_some_and(|ending| ENDINGS.contains(&ending))
    }
}

/// The words a question asks about, each once, in its order: names and their parts, without the
/// words every question has (`where`, `the`, `file`) and none shorter than two letters.
pub(super) fn terms(question: &str) -> Vec<Term> {
    let mut terms: Vec<Term> = vec![];
    let mut add = |word: String| {
        if word.len() < 2 || STOP.contains(&word.as_str()) {
            return;
        }
        let term = term(word);
        if !terms.iter().any(|known| known.stem == term.stem) {
            terms.push(term);
        }
    };
    for word in words(question) {
        let parts = parts(word);
        if parts.len() > 1 {
            add(word.to_ascii_lowercase());
        }
        for part in parts {
            add(part.to_ascii_lowercase());
        }
    }
    terms
}

/// Whether the question asks where something is defined: a definition's line then counts more.
pub(super) fn asks_for_definition(question: &str) -> bool {
    words(question).any(|word| {
        let word = word.to_ascii_lowercase();
        word.starts_with("defin") || word == "declared" || word == "declares"
    })
}

/// Each word of `text`, lower-cased, as [`Term::finds`] reads it: the whole name, then each of its
/// parts when it has more than one.
pub(super) fn each_word(text: &str, mut found: impl FnMut(&str)) {
    let mut lower = String::new();
    for word in words(text) {
        let parts = parts(word);
        if parts.len() > 1 {
            lowered(word, &mut lower);
            found(&lower);
        }
        for part in parts {
            lowered(part, &mut lower);
            found(&lower);
        }
    }
}

fn lowered(word: &str, into: &mut String) {
    into.clear();
    into.extend(word.chars().map(|c| c.to_ascii_lowercase()));
}

/// The runs of letters, digits and `_` in `text`.
fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
}

/// A name's parts: split at `_`, and where a lower letter or a digit meets an upper one
/// (`parseRows`), or an upper run meets a word (`HTTPServer` is `HTTP`, `Server`).
fn parts(word: &str) -> Vec<&str> {
    let mut parts = vec![];
    for piece in word.split('_').filter(|piece| !piece.is_empty()) {
        let chars: Vec<(usize, char)> = piece.char_indices().collect();
        let mut start = 0;
        for (n, window) in chars.windows(2).enumerate() {
            let [(_, before), (at, now)] = [window[0], window[1]];
            let after = chars.get(n + 2).map(|(_, c)| *c);
            let camel = (before.is_lowercase() || before.is_ascii_digit()) && now.is_uppercase();
            let acronym = before.is_uppercase()
                && now.is_uppercase()
                && after.is_some_and(char::is_lowercase);
            if camel || acronym {
                parts.extend(piece.get(start..at));
                start = at;
            }
        }
        parts.extend(piece.get(start..));
    }
    parts
}

/// `word` as a term: a number by its value's digits, short words whole, longer ones by their
/// stem, so an inflection is found.
fn term(word: String) -> Term {
    if word.bytes().all(|b| b.is_ascii_digit()) {
        return Term {
            stem: number(&word),
            word,
            exact: true,
            number: true,
            pattern: None,
        };
    }
    let exact = word.chars().count() <= 3;
    Term {
        stem: if exact { word.clone() } else { stem(&word) },
        word,
        exact,
        number: false,
        pattern: None,
    }
}

/// A number's digits without its radix's prefix and its leading zeros: `0o600` and `0600` are
/// `600`.
fn number(word: &str) -> String {
    let digits = ["0o", "0x", "0b"]
        .iter()
        .find_map(|prefix| word.strip_prefix(prefix))
        .unwrap_or(word);
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        "0".into()
    } else {
        digits.into()
    }
}

/// `word` without the common English endings, kept at three letters at least: `kills` is
/// `kill`, `started` `start`, `retries` `retry`, `processes` `process`.
fn stem(word: &str) -> String {
    let cut = |suffix: &str, keep: &str| -> Option<String> {
        let stem = word.strip_suffix(suffix)?;
        (stem.chars().count() >= 3).then(|| format!("{stem}{keep}"))
    };
    let stemmed = cut("ies", "y")
        .or_else(|| cut("sses", "ss"))
        .or_else(|| cut("ing", ""))
        .or_else(|| cut("ed", ""))
        .or_else(|| {
            [("ches", "ch"), ("shes", "sh"), ("xes", "x")]
                .iter()
                .find_map(|(s, k)| cut(s, k))
        })
        .or_else(|| (!word.ends_with("ss")).then(|| cut("s", "")).flatten());
    stemmed.unwrap_or_else(|| word.to_string())
}

/// The endings an English word takes when it is inflected, on its stem.
const ENDINGS: &[&str] = &[
    "", "s", "e", "es", "ed", "d", "ing", "ies", "y", "ure", "ures", "ion", "ions", "ation",
];

/// The words a code search question has whatever it asks.
const STOP: &[&str] = &[
    "a",
    "about",
    "after",
    "all",
    "an",
    "and",
    "any",
    "are",
    "as",
    "at",
    "be",
    "been",
    "before",
    "but",
    "by",
    "can",
    "code",
    "codebase",
    "could",
    "declared",
    "declares",
    "define",
    "defined",
    "defines",
    "definition",
    "do",
    "does",
    "doing",
    "done",
    "each",
    "every",
    "file",
    "files",
    "find",
    "for",
    "from",
    "function",
    "functions",
    "get",
    "gets",
    "has",
    "have",
    "how",
    "if",
    "implemented",
    "in",
    "into",
    "is",
    "it",
    "its",
    "kind",
    "line",
    "lines",
    "look",
    "made",
    "make",
    "makes",
    "many",
    "me",
    "method",
    "module",
    "more",
    "most",
    "my",
    "no",
    "not",
    "of",
    "on",
    "one",
    "or",
    "other",
    "our",
    "out",
    "over",
    "part",
    "place",
    "places",
    "project",
    "repo",
    "repository",
    "same",
    "should",
    "show",
    "so",
    "some",
    "such",
    "than",
    "that",
    "the",
    "their",
    "them",
    "then",
    "there",
    "these",
    "they",
    "thing",
    "this",
    "those",
    "through",
    "to",
    "under",
    "up",
    "use",
    "used",
    "uses",
    "using",
    "via",
    "was",
    "way",
    "we",
    "were",
    "what",
    "when",
    "where",
    "whether",
    "which",
    "while",
    "who",
    "why",
    "will",
    "with",
    "within",
    "would",
    "you",
];

#[cfg(test)]
#[path = "terms_tests.rs"]
mod tests;
