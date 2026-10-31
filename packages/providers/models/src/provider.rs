//! A provider as the catalog lists it: its name and key variables read with the document, each of
//! its models decoded the first time it is asked for. A refreshed catalog lists thousands of
//! models and a run uses a handful, so reading it is an index of raw JSON, not a decode of all of
//! it (a start of 50 ms and 60 MB for nothing, in a daemon for as long as it lives).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::types::Model;

/// A provider, as the v2 API publishes it.
#[derive(Debug, Clone, Deserialize)]
pub struct Provider {
    name: String,
    #[serde(default)]
    env: Vec<String>,
    /// Its models as the document has them: one that does not read as a model is left out when
    /// it is asked for, not the whole catalog.
    #[serde(default)]
    models: BTreeMap<String, Slot>,
}

impl Provider {
    /// What the person reads.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The environment variables its key is read from.
    pub fn env(&self) -> &[String] {
        &self.env
    }

    /// Its model `id`, when it has one that reads as a model.
    pub fn model(&self, id: &str) -> Option<&Model> {
        self.models.get(id)?.get()
    }

    /// Its models that read as models, by id; each decoded as the iteration reaches it.
    pub fn models(&self) -> impl Iterator<Item = (&str, &Model)> {
        let slots = self.models.iter();
        slots.filter_map(|(id, slot)| Some((id.as_str(), slot.get()?)))
    }

    /// It as a document to keep, with only the models `keep` takes; `None` when it takes none.
    pub(crate) fn kept(&self, keep: impl Fn(&Model) -> bool) -> Option<Kept<'_>> {
        let models: BTreeMap<&str, &Model> = self.models().filter(|(_, m)| keep(m)).collect();
        (!models.is_empty()).then(|| Kept {
            name: &self.name,
            env: &self.env,
            models,
        })
    }
}

/// A provider as a refresh keeps it, in the document's own shape.
#[derive(Serialize)]
pub(crate) struct Kept<'a> {
    name: &'a str,
    env: &'a [String],
    models: BTreeMap<&'a str, &'a Model>,
}

/// A model as the document has it, and what it reads as once asked (boxed: a slot never asked
/// for costs a pointer, not a model's size).
#[derive(Debug, Clone, Deserialize)]
#[serde(from = "Box<RawValue>")]
struct Slot {
    raw: Box<RawValue>,
    model: OnceLock<Option<Box<Model>>>,
}

impl From<Box<RawValue>> for Slot {
    fn from(raw: Box<RawValue>) -> Self {
        Self {
            raw,
            model: OnceLock::new(),
        }
    }
}

impl Slot {
    fn get(&self) -> Option<&Model> {
        let read = || serde_json::from_str(self.raw.get()).ok();
        self.model.get_or_init(read).as_deref()
    }
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;
