//! The `result` call of `--schema`: offered with every request of the task, so the tools, the
//! start of every prompt, never change and the prompt cache holds to the end. A `result` alone,
//! its data matching, ends the task (`agent/returned.rs`); an answer in words is followed by one
//! more request on the same conversation, the call forced (`Calling::Only`), so the data comes
//! as a call, deterministically. Data that does not match goes back to be fixed, and what is
//! returned always matches or the run says it has no data. The check is the app's, through the
//! `Shape` port (ARCHITECTURE.md P8).

use contracts::shape::Shape;
use contracts::tool::{Input, ToolSpec};
use serde_json::{Value, json};

pub(crate) const TOOL: &str = "result";

/// The `result` tool carrying `data` for `schema`.
pub(crate) fn spec(schema: &Value) -> ToolSpec {
    ToolSpec {
        name: TOOL.into(),
        description: "The result of the task as `data`, matching its schema exactly: copy \
            numbers, names and dates from what the tools showed, null only where the schema \
            allows it and no value was shown. Call it alone, once the work is done: it ends \
            the task."
            .into(),
        input: Input::Json(json!({
            "type": "object",
            "required": ["data"],
            "properties": {"data": schema.clone()},
        })),
    }
}

/// The data of a `result` call, checked against `schema`; or what to fix.
pub(crate) fn take(args: &Value, schema: &dyn Shape) -> Result<Value, String> {
    let data = match args.get("data") {
        Some(data) => data.clone(),
        None => beside(args, schema)?,
    };
    check(schema, &data)?;
    Ok(data)
}

/// The schema's fields written at the top of the call instead of inside `data` (a schema with a
/// `summary` of its own, as a pull request's description has): the fields are the data when they
/// match the schema, and otherwise the model is told where they go.
fn beside(args: &Value, schema: &dyn Shape) -> Result<Value, String> {
    let data = Value::Object(args.as_object().cloned().unwrap_or_default());
    check(schema, &data)
        .map_err(|why| format!("no `data`: put the schema's fields inside `data` ({why})"))?;
    Ok(data)
}

/// Whether `data` matches `schema`, or every way it does not.
fn check(schema: &dyn Shape, data: &Value) -> Result<(), String> {
    let problems = schema.mismatches(data);
    match problems.is_empty() {
        true => Ok(()),
        false => Err(problems.join("; ")),
    }
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
