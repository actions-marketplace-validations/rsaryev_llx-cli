//! The patterns that need no automaton, checked by one comparison or one scan: one that every
//! text matches (`.*`, `a*`, `^`), words (`^x-`, `^(small|medium|large)$`, `foo`), and one class
//! counted over the whole text (`^[a-zA-Z0-9]+$`, `^\d{4}$`, `^.{1,255}$`), or over a run of it
//! where its anchors put one (`[a-zA-Z0-9_-]{162}`, `\d+`, `^[a-z]+`).

use std::slice;

use super::class::Class;
use super::parse::Node;
use super::shape::{Branch, nullable};

/// A pattern checked without an automaton.
pub(super) enum Simple {
    /// Every text matches: an alternative matches the empty text where it may start or end.
    Always,
    /// Any of the words, each where its anchors put it.
    Words(Vec<Word>),
    /// A run of one class, its length in characters within the bounds: the whole text, or at
    /// least `min` characters where the anchors put it.
    Run(Run),
}

pub(super) struct Word {
    text: String,
    start: bool,
    end: bool,
}

pub(super) struct Run {
    /// The ASCII characters of the class, bit `c` for `c`: most text is checked by it alone.
    ascii: u128,
    class: Class,
    min: u32,
    max: Option<u32>,
    start: bool,
    end: bool,
}

/// The simple form of a pattern of these alternatives, when it has one.
pub(super) fn simple(branches: &[Branch<'_>]) -> Option<Simple> {
    let always = branches
        .iter()
        .any(|branch| !(branch.start && branch.end) && branch.body.iter().all(nullable));
    if always {
        return Some(Simple::Always);
    }
    // One character not held at both ends is a word: looked for as one, it is a `memchr`.
    if let [branch] = branches
        && (branch.start && branch.end || literal(branch.body).is_none())
        && let Some(run) = run(branch)
    {
        return Some(Simple::Run(run));
    }
    let mut words = vec![];
    for branch in branches {
        // `^(small|medium|large)$`: a group of words between the anchors is a word each.
        if let [Node::Alt(alternatives)] = branch.body {
            for alternative in alternatives {
                words.push(word(branch, literal(slice::from_ref(alternative))?));
            }
        } else {
            words.push(word(branch, literal(branch.body)?));
        }
    }
    Some(Simple::Words(words))
}

fn word(branch: &Branch<'_>, text: String) -> Word {
    Word {
        text,
        start: branch.start,
        end: branch.end,
    }
}

/// The text `nodes` match, when they match one text only.
fn literal(nodes: &[Node]) -> Option<String> {
    let mut text = String::new();
    for node in nodes {
        match node {
            Node::Class(class) => text.push(class.single()?),
            Node::Concat(items) => text.push_str(&literal(items)?),
            Node::Alt(_) | Node::Repeat(..) | Node::Assert(_) => return None,
        }
    }
    Some(text)
}

fn run(branch: &Branch<'_>) -> Option<Run> {
    let (class, min, max) = match branch.body {
        [Node::Class(class)] => (class, 1, Some(1)),
        [Node::Repeat(inner, min, max)] => match inner.as_ref() {
            Node::Class(class) => (class, *min, *max),
            Node::Concat(_) | Node::Alt(_) | Node::Repeat(..) | Node::Assert(_) => return None,
        },
        _ => return None,
    };
    Some(Run {
        ascii: class.ascii(),
        class: class.clone(),
        min,
        max,
        start: branch.start,
        end: branch.end,
    })
}

impl Simple {
    pub(super) fn is_match(&self, text: &str) -> bool {
        match self {
            Self::Always => true,
            Self::Words(words) => words.iter().any(|word| word.is_match(text)),
            Self::Run(run) => run.is_match(text),
        }
    }
}

impl Word {
    fn is_match(&self, text: &str) -> bool {
        match (self.start, self.end) {
            (true, true) => text == self.text,
            (true, false) => text.starts_with(&self.text),
            (false, true) => text.ends_with(&self.text),
            (false, false) => text.contains(&self.text),
        }
    }
}

impl Run {
    fn is_match(&self, text: &str) -> bool {
        // A run held at both ends is the whole text, whose length must stay within the bounds;
        // any other needs `min` characters only, as a match may stop before a longer run ends.
        let whole = self.start && self.end;
        // A character is a byte at least: a shorter text holds no run long enough.
        if text.len() < usize::try_from(self.min).unwrap_or(usize::MAX) {
            return false;
        }
        let max = self.max.filter(|_| whole).unwrap_or(u32::MAX);
        // The count only needs to reach the bounds: one held at u32::MAX is as good.
        let mut count: u32 = 0;
        let bytes = text.as_bytes();
        let mut at = 0;
        while let Some(&byte) = bytes.get(at) {
            let held = if byte < 0x80 {
                at += 1;
                self.ascii >> byte & 1 != 0
            } else {
                let Some(c) = text.get(at..).and_then(|rest| rest.chars().next()) else {
                    return false;
                };
                at += c.len_utf8();
                self.class.contains(c)
            };
            if held {
                count = count.saturating_add(1);
            } else if self.start {
                return false;
            } else {
                count = 0;
            }
            if count > max {
                return false;
            }
            if !self.end && count >= self.min {
                return true;
            }
        }
        count >= self.min
    }
}

#[cfg(test)]
#[path = "simple_tests.rs"]
mod tests;
