//! Files ranked by the question's words, with no model: BM25 over each file's words (a rare word
//! counts more than a common one, a long file is not favoured for its length), a word in the
//! path or on a definition's line counting more, and a file that has more of the question's words
//! ahead of one that repeats a few.

use super::listing::Doc;
use super::terms::{self, Term};

/// A file's place in the ranking.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Scored {
    /// Its index among the files read.
    pub(super) doc: usize,
    pub(super) score: f64,
}

/// What of the question one line of a file has: a bit per term.
pub(super) type Mask = u32;

/// At most this many terms are looked for: a mask's bits.
pub(super) const MOST_TERMS: usize = 32;

/// BM25's saturation and length normalisation, the usual values.
const K1: f64 = 1.2;
const B: f64 = 0.75;
/// What a term in the path is worth, against one occurrence in the text.
const IN_PATH: f64 = 1.5;
/// What a term on a definition's line is worth, by whether the question asks for a definition.
const ON_DEFINITION: [f64; 2] = [0.5, 3.0];
/// A test, a fixture, a spec or a directory of data answers a question about the code less often
/// than the code does.
const ASIDE_WEIGHT: f64 = 0.5;

/// One file's counts.
struct Counts {
    each: Vec<u32>,
    words: u32,
    path: Mask,
    defined: Mask,
}

/// Every file that has any of `terms`, best first, and what each term is worth (its IDF).
pub(super) fn ranked(docs: &[Doc], terms: &[Term], definition: bool) -> (Vec<Scored>, Vec<f64>) {
    let counts: Vec<Counts> = docs.iter().map(|doc| counted(doc, terms)).collect();
    let n = docs.len().max(1) as f64;
    let idf: Vec<f64> = (0..terms.len())
        .map(|t| {
            let df = counts.iter().filter(|c| c.each[t] > 0).count() as f64;
            (1.0 + (n - df + 0.5) / (df + 0.5)).ln()
        })
        .collect();
    let total: f64 = idf.iter().sum::<f64>().max(f64::MIN_POSITIVE);
    let mean = (counts.iter().map(|c| f64::from(c.words)).sum::<f64>() / n).max(1.0);
    let defined = ON_DEFINITION[usize::from(definition)];
    let mut scored: Vec<Scored> = (counts.iter().enumerate())
        .filter_map(|(doc, c)| {
            let length = f64::from(c.words) / mean;
            let (mut score, mut covered) = (0.0, 0.0);
            for (t, idf) in idf.iter().enumerate() {
                let tf = f64::from(c.each[t]);
                let bit = 1 << t;
                let in_path = c.path & bit != 0;
                if tf == 0.0 && !in_path {
                    continue;
                }
                covered += idf;
                score += idf * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * length));
                score += idf * IN_PATH * f64::from(u8::from(in_path));
                score += idf * defined * f64::from(u8::from(c.defined & bit != 0));
            }
            let path = &docs.get(doc)?.path;
            let weight = if is_test(path) || in_data(path) {
                ASIDE_WEIGHT
            } else {
                1.0
            };
            (score > 0.0).then(|| Scored {
                doc,
                score: score * (covered / total) * weight,
            })
        })
        .collect();
    scored.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.doc.cmp(&b.doc)));
    (scored, idf)
}

/// Each line of `text` that has any of `terms`: its index and what it has.
pub(super) fn hits(text: &str, terms: &[Term]) -> Vec<(usize, Mask)> {
    (text.lines().enumerate())
        .filter_map(|(at, line)| {
            let mask = line_mask(line, terms, &mut 0, &mut []);
            (mask != 0).then_some((at, mask))
        })
        .collect()
}

/// Whether `line` declares something: a function, a type, a constant, a module, a heading.
pub(super) fn defines(line: &str) -> bool {
    let mut rest = line.trim_start();
    if rest.starts_with('#') && !rest.starts_with("#[") && !rest.starts_with("#!") {
        return true;
    }
    for modifier in [
        "pub(crate) ",
        "pub(super) ",
        "pub ",
        "export ",
        "default ",
        "async ",
        "unsafe ",
    ] {
        if let Some(after) = rest.strip_prefix(modifier)
            && !after.starts_with('{')
        {
            rest = after;
        }
    }
    DECLARES.iter().any(|keyword| {
        rest.strip_prefix(keyword)
            .is_some_and(|after| after.starts_with(' ') || after.starts_with('!'))
    })
}

const DECLARES: &[&str] = &[
    "fn",
    "struct",
    "enum",
    "trait",
    "impl",
    "type",
    "const",
    "static",
    "mod",
    "macro_rules",
    "class",
    "def",
    "function",
    "interface",
    "func",
];

fn counted(doc: &Doc, terms: &[Term]) -> Counts {
    let mut counts = Counts {
        each: vec![0; terms.len()],
        words: 0,
        path: line_mask(&doc.path, terms, &mut 0, &mut []),
        defined: 0,
    };
    for line in doc.text.lines() {
        let mask = line_mask(line, terms, &mut counts.words, &mut counts.each);
        if mask != 0 && defines(line) {
            counts.defined |= mask;
        }
    }
    counts
}

/// The terms `line` has, its words added to `words` and each term's count to `each` (when
/// given: empty leaves them uncounted).
fn line_mask(line: &str, terms: &[Term], words: &mut u32, each: &mut [u32]) -> Mask {
    let mut mask = 0;
    terms::each_word(line, |word| {
        *words = words.saturating_add(1);
        for (t, term) in terms.iter().enumerate() {
            if term.finds(word) {
                mask |= 1 << t;
                if let Some(count) = each.get_mut(t) {
                    *count = count.saturating_add(1);
                }
            }
        }
    });
    mask
}

/// A file under a directory the usual layouts keep data, vendored or generated files in.
fn in_data(path: &str) -> bool {
    path.split('/').rev().skip(1).any(|dir| {
        [
            "data",
            "vendor",
            "third_party",
            "generated",
            "snapshots",
            "__snapshots__",
        ]
        .contains(&dir)
    })
}

/// A test's file, by the names the usual layouts give one.
pub(super) fn is_test(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    [
        "/tests/",
        "/test/",
        "/__tests__/",
        "/spec/",
        "/fixtures/",
        "/testdata/",
    ]
    .iter()
    .any(|dir| path.contains(dir) || dir.get(1..).is_some_and(|dir| path.starts_with(dir)))
        || [
            "_tests.rs",
            "_test.rs",
            "_test.go",
            ".test.ts",
            ".test.js",
            ".spec.ts",
            ".spec.js",
            "_test.py",
        ]
        .iter()
        .any(|end| path.ends_with(end))
        || path
            .rsplit('/')
            .next()
            .is_some_and(|name| name.starts_with("test_"))
}

#[cfg(test)]
#[path = "rank_tests.rs"]
mod tests;
