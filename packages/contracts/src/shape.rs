//! The shape a run's result must have (`--schema`): the JSON Schema the model is told, and every
//! way data does not match it. The app supplies it (from the `schema` primitive, ARCHITECTURE.md
//! P8): the agent holds no JSON Schema of its own.

use serde_json::Value;

/// A JSON Schema and its check.
pub trait Shape: Send + Sync {
    /// The schema, as the model is told it.
    fn schema(&self) -> &Value;
    /// Every way `data` does not match, each a line the model can fix it by (`data.price must be
    /// number (is string)`); none when it matches.
    fn mismatches(&self, data: &Value) -> Vec<String>;
}
