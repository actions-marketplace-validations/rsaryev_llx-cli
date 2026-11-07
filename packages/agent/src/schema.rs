//! The `result` call of `--schema`: offered with every request of the task, so the tools, the
//! start of every prompt, never change and the prompt cache holds to the end. A `result` alone,
//! its data matching, ends the task (`agent/returned.rs`); an answer in words is followed by one
//! more request on the same conversation, the call forced (`Calling::Only`), so the data comes
//! as a call, deterministically. Data that does not match goes back to be fixed, and what is
//! returned always matches or the run says it has no data.

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
pub(crate) fn take(args: &Value, schema: &Value) -> Result<Value, String> {
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
fn beside(args: &Value, schema: &Value) -> Result<Value, String> {
    let data = Value::Object(args.as_object().cloned().unwrap_or_default());
    check(schema, &data)
        .map_err(|why| format!("no `data`: put the schema's fields inside `data` ({why})"))?;
    Ok(data)
}

/// Whether `data` matches `schema`, or every way it does not.
fn check(schema: &Value, data: &Value) -> Result<(), String> {
    let mut problems = vec![];
    violations(schema, data, "data", &mut problems);
    match problems.is_empty() {
        true => Ok(()),
        false => Err(problems.join("; ")),
    }
}

/// Where `value`, at `at` in the call, breaks `schema`: type, required properties, enum, items,
/// nested objects.
fn violations(schema: &Value, value: &Value, at: &str, out: &mut Vec<String>) {
    if let Some(options) = schema.get("enum").and_then(Value::as_array)
        && !options.contains(value)
    {
        out.push(format!(
            "{at} must be one of {}",
            Value::Array(options.clone())
        ));
    }
    let types: Vec<&str> = match schema.get("type") {
        Some(Value::String(one)) => vec![one.as_str()],
        Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    };
    if !types.is_empty() && !types.iter().any(|kind| matches_type(kind, value)) {
        out.push(format!("{at} must be {}", types.join(" or ")));
        return;
    }
    if let Some(object) = value.as_object() {
        // `required` stands on its own: a schema may list no `properties` beside it.
        let required = schema.get("required").and_then(Value::as_array);
        for name in required.into_iter().flatten().filter_map(Value::as_str) {
            if !object.contains_key(name) {
                out.push(format!("{at} is missing \"{name}\""));
            }
        }
        let properties = schema.get("properties").and_then(Value::as_object);
        for (name, sub) in properties.into_iter().flatten() {
            if let Some(child) = object.get(name) {
                violations(sub, child, &format!("{at}.{name}"), out);
            }
        }
    }
    if let (Some(items), Some(schema_items)) = (value.as_array(), schema.get("items")) {
        for (i, item) in items.iter().enumerate() {
            violations(schema_items, item, &format!("{at}[{i}]"), out);
        }
    }
}

fn matches_type(kind: &str, value: &Value) -> bool {
    match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.as_f64().is_some_and(|n| n.fract() == 0.0),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
