//! `--schema` as the agent checks it: the `schema` primitive behind the `Shape` port
//! (ARCHITECTURE.md P8).

use contracts::shape::Shape;
use schema::Schema;
use serde_json::Value;

/// The schema `--schema` gives, and its check.
pub(crate) struct Checked(Schema);

impl Checked {
    pub(crate) fn new(json: Value) -> Self {
        Self(Schema::new(json))
    }

    /// What `--schema` says data is not held to here, said as the run starts: the model is told
    /// those keywords, and a reader of the data should not count on them.
    pub(crate) fn unchecked_note(&self) -> Option<String> {
        let unchecked = self.0.unchecked();
        let (are, them) = match unchecked.len() {
            1 => ("is", "it"),
            _ => ("are", "them"),
        };
        (!unchecked.is_empty()).then(|| {
            let names = unchecked.join(", ");
            format!(
                "llx: --schema: {names} {are} not checked (the model is told {them}; the data is not held to {them})\n"
            )
        })
    }
}

impl Shape for Checked {
    fn schema(&self) -> &Value {
        self.0.json()
    }

    fn mismatches(&self, data: &Value) -> Vec<String> {
        self.0.mismatches(data)
    }
}

#[cfg(test)]
#[path = "shape_tests.rs"]
mod tests;
