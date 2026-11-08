//! Data checked against a JSON Schema given at run time (`--schema`), tuned for what a language
//! model writes: drafts 4, 6, 7, 2019-09 and 2020-12, read by the document's `$schema`
//! (`draft.rs`), compiled once (`compile.rs`, its references through `index.rs` and `uri.rs`)
//! and walked for each check (`validate.rs`), valid data with no allocation. Formats are checked,
//! not only noted (`format.rs`), and a pattern by its own matcher (`pattern.rs`). Equality is
//! JSON's, numbers compare exactly (`number.rs`, `hash.rs`). What a schema asks that is not
//! checked here is named by [`Schema::unchecked`], never dropped unsaid.
//!
//! The README follows, its example run as a test.
#![doc = include_str!("../README.md")]

mod compile;
mod draft;
mod format;
mod hash;
mod index;
mod node;
mod number;
mod pattern;
mod report;
mod uri;
mod validate;

use serde_json::Value;

use node::Node;
use pattern::Pattern;
use report::{At, Report};
use validate::Walk;

/// A JSON Schema, as given: an object, or `true` or `false`.
pub struct Schema {
    json: Value,
    /// The schema compiled, its root the first node.
    nodes: Vec<Node>,
    /// Its patterns: each source, and its matcher where it is checked here.
    patterns: Vec<(Box<str>, Option<Pattern>)>,
    /// What it asks that is not checked here.
    notes: Vec<&'static str>,
    /// Draft 4's integers.
    strict: bool,
}

impl Schema {
    pub fn new(json: Value) -> Self {
        Self::with_documents(json, &[])
    }

    /// A schema whose `$ref`s may land in other documents, each given with its URI
    /// (`https://example.com/defs.json`): no network is ever reached.
    pub fn with_documents(json: Value, documents: &[(String, Value)]) -> Self {
        let compiled = compile::compile(&json, documents);
        let mut notes = compiled.notes;
        let patterns: Vec<(Box<str>, Option<Pattern>)> = (compiled.patterns.into_iter())
            .map(|source| {
                let matcher = Pattern::new(&source).ok();
                (source, matcher)
            })
            .collect();
        if patterns.iter().any(|(_, matcher)| matcher.is_none()) {
            notes.push("pattern");
        }
        Self {
            json,
            nodes: compiled.nodes,
            patterns,
            notes,
            strict: compiled.draft.strict_integers(),
        }
    }

    /// The schema as given.
    pub fn json(&self) -> &Value {
        &self.json
    }

    fn walk(&self) -> Walk<'_> {
        Walk {
            nodes: &self.nodes,
            patterns: &self.patterns,
            strict: self.strict,
        }
    }

    /// Whether `data` matches: the quick check, which stops at the first failure and says nothing.
    pub fn matches(&self, data: &Value) -> bool {
        self.walk().node(0, data, &At::Root, Report::quiet(), 0)
    }

    /// Every way `data` does not match, the first twenty, its places written from `data`
    /// (`data.legs[0] is missing "from"`); none when it matches.
    pub fn mismatches(&self, data: &Value) -> Vec<String> {
        // Valid data is the common case: the quick walk says so, and only a failure is walked
        // again for every message.
        if self.matches(data) {
            return vec![];
        }
        let mut out = vec![];
        (self.walk()).node(0, data, &At::Root, Report::said(&mut out), 0);
        out
    }

    /// What the schema asks that is not checked here, each named once.
    pub fn unchecked(&self) -> Vec<&'static str> {
        self.notes.clone()
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
