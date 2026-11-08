//! Typical checks of an LLM's structured answer, each against `jsonschema`: fixed schemas and
//! data, the median of 31 rounds, exact allocation counts. Prints a Markdown table.

use schema::Schema;
use schema_bench::{measure, reference, shown};
use serde_json::{Map, Value, json};

/// A code review's verdict, as `llx exec --schema` asks for one.
fn verdict() -> Value {
    json!({"type": "object", "required": ["verdict", "summary", "issues"],
        "additionalProperties": false,
        "properties": {
            "verdict": {"enum": ["approve", "request_changes", "comment"]},
            "summary": {"type": "string", "minLength": 1, "maxLength": 2000},
            "issues": {"type": "array", "maxItems": 50, "items": {"type": "object",
                "required": ["file", "line", "severity", "message"],
                "properties": {"file": {"type": "string"}, "line": {"type": "integer", "minimum": 1},
                    "severity": {"enum": ["low", "medium", "high"]},
                    "message": {"type": "string"}}}}}})
}

fn issues(count: usize, broken: bool) -> Value {
    let list: Vec<Value> = (0..count)
        .map(|i| match broken && i % 7 == 0 {
            true => json!({"file": "src/a.rs", "line": 0, "severity": "urgent", "message": 5}),
            false => json!({"file": format!("src/f{i}.rs"), "line": i + 1,
                "severity": (["low", "medium", "high"][i % 3]), "message": "Something to fix."}),
        })
        .collect();
    json!({"verdict": "request_changes", "summary": "Two problems in the parser.", "issues": list})
}

/// Rows of a table an answer lists.
fn rows() -> (Value, Value) {
    let schema = json!({"type": "array", "items": {"type": "object",
        "required": ["id", "name", "price", "tags"],
        "properties": {"id": {"type": "integer"}, "name": {"type": "string"},
            "price": {"type": "number"}, "stock": {"type": ["integer", "null"]},
            "tags": {"type": "array", "items": {"type": "string"}},
            "kind": {"enum": ["a", "b", "c"]}}}});
    let data = (0..1000)
        .map(|i| {
            json!({"id": i, "name": format!("item {i}"), "price": f64::from(i) * 1.5,
            "stock": if i % 2 == 0 { json!(i) } else { Value::Null },
            "tags": ["x", "y", "z"], "kind": (["a", "b", "c"][(i % 3) as usize])})
        })
        .collect();
    (schema, Value::Array(data))
}

/// Objects of twenty fields each, all required, nothing else allowed.
fn wide() -> (Value, Value) {
    let name = |i: usize| format!("field_{i:02}");
    let properties: Map<String, Value> = (0..20)
        .map(|i| {
            (
                name(i),
                json!({"type": if i % 2 == 0 { "string" } else { "integer" }}),
            )
        })
        .collect();
    let schema = json!({"type": "array", "items": {"type": "object",
        "required": (0..20).map(name).collect::<Vec<_>>(),
        "additionalProperties": false, "properties": properties}});
    let row = |r: usize| -> Value {
        let fields: Map<String, Value> = (0..20)
            .map(|i| {
                (
                    name(i),
                    if i % 2 == 0 {
                        json!(format!("v{r}"))
                    } else {
                        json!(r * i)
                    },
                )
            })
            .collect();
        Value::Object(fields)
    };
    (schema, Value::Array((0..500).map(row).collect()))
}

/// A tree through a recursive `$ref`.
fn tree() -> (Value, Value) {
    fn node(depth: usize) -> Value {
        match depth {
            0 => json!({"name": "leaf"}),
            _ => {
                json!({"name": format!("n{depth}"), "children": [node(depth - 1), node(depth - 1)]})
            }
        }
    }
    let schema = json!({"$defs": {"node": {"type": "object", "required": ["name"],
        "properties": {"name": {"type": "string"},
            "children": {"type": "array", "items": {"$ref": "#/$defs/node"}}}}},
        "$ref": "#/$defs/node"});
    (schema, node(8))
}

/// A union of tool calls told apart by `type`, as function calling writes them.
fn calls() -> (Value, Value) {
    let tools = [
        "read", "write", "run", "search", "ask", "fetch", "list", "delete",
    ];
    let branches: Vec<Value> = tools
        .iter()
        .map(|tool| {
            json!({"type": "object", "required": ["type", "arguments"],
            "additionalProperties": false,
            "properties": {"type": {"const": tool}, "arguments": {"type": "object",
                "required": ["target"], "properties": {"target": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 0}}}}})
        })
        .collect();
    let schema = json!({"type": "array", "items": {"oneOf": branches}});
    let data = (0..64)
        .map(|i| json!({"type": tools[i % tools.len()], "arguments": {"target": format!("t{i}")}}))
        .collect();
    (schema, Value::Array(data))
}

fn strings(prefix: &str, count: usize) -> Vec<Value> {
    (0..count).map(|i| json!(format!("{prefix}-{i}"))).collect()
}

/// One row: what the check is, ours and the reference timed, and how far apart they are.
fn row(name: &str, schema: &Value, data: &Value) {
    let ours = Schema::new(schema.clone());
    let Some(theirs) = reference(schema) else {
        println!("| {name} | – | – | jsonschema refused the schema |");
        return;
    };
    assert_eq!(
        ours.matches(data),
        theirs.is_valid(data),
        "{name}: the verdicts differ"
    );
    let a = measure(|| ours.matches(data));
    let b = measure(|| theirs.is_valid(data));
    println!(
        "| {name} | {} ({} alloc) | {} ({} alloc) | {:.2}x |",
        shown(a.ns),
        a.allocations,
        shown(b.ns),
        b.allocations,
        b.ns / a.ns,
    );
}

fn main() {
    println!("| check | schema | jsonschema 0.60 | ours faster by |");
    println!("|---|---|---|---|");
    let review = verdict();
    row("review verdict, valid", &review, &issues(5, false));
    row("review verdict, invalid", &review, &issues(5, true));
    let (schema, data) = rows();
    row("1,000 rows", &schema, &data);
    let (schema, data) = wide();
    row("500 objects × 20 fields", &schema, &data);
    let (schema, data) = tree();
    row("tree of 511 nodes by `$ref`", &schema, &data);
    let (schema, data) = calls();
    row("64 tool calls, `oneOf` of 8", &schema, &data);
    let unique = json!({"type": "array", "uniqueItems": true});
    row(
        "`uniqueItems`, 1,000 strings",
        &unique,
        &Value::Array(strings("item", 1000)),
    );
    let options = json!({"enum": strings("option", 1000)});
    row("`enum` of 1,000, the last", &options, &json!("option-999"));
    for (format, value) in [
        ("date-time", "2026-11-07T18:39:00Z"),
        ("email", "a.b@example.com"),
        ("uuid", "550e8400-e29b-41d4-a716-446655440000"),
        ("ipv4", "192.168.0.1"),
        ("uri", "https://example.com/a?b=c#d"),
    ] {
        row(
            &format!("`format: {format}`"),
            &json!({"format": format}),
            &json!(value),
        );
    }
    for (pattern, value) in [
        ("^[A-Z]{3}$", "USD"),
        ("^\\d{4}-\\d{2}-\\d{2}$", "2026-11-07"),
        (
            "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$",
            "a.b@example.com",
        ),
        ("^(small|medium|large)$", "medium"),
    ] {
        row(
            &format!("`pattern: {pattern}`"),
            &json!({"pattern": pattern}),
            &json!(value),
        );
    }
    println!();
    println!("| compiled once | ours | jsonschema 0.60 | ours faster by |");
    println!("|---|---|---|---|");
    for (name, schema) in [
        ("review verdict", review.clone()),
        ("tool calls", calls().0),
    ] {
        let a = measure(|| Schema::new(schema.clone()));
        let b = measure(|| reference(&schema).is_some());
        let copy = measure(|| schema.clone());
        let ours = a.ns - copy.ns;
        println!(
            "| {name} | {} | {} | {:.2}x |",
            shown(ours),
            shown(b.ns),
            b.ns / ours
        );
    }
    let broken = issues(5, true);
    let ours = Schema::new(review.clone());
    if let Some(theirs) = reference(&review) {
        let a = measure(|| ours.mismatches(&broken).len());
        // Both write every message out as text, as the model reads them.
        let b = measure(|| theirs.iter_errors(&broken).map(|e| e.to_string()).count());
        println!();
        println!(
            "Every message of an invalid verdict: ours {}, jsonschema {} ({:.2}x).",
            shown(a.ns),
            shown(b.ns),
            b.ns / a.ns
        );
    }
}
