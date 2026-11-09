//! models.dev's files in `data/` (MODELS.md), resolved as models.dev resolves them, into one
//! document in the shape of its v2 API, cut by the catalog's types (`src/types.rs`) as a fetched
//! catalog is, built into the crate. The rules (models.dev `generate.ts` and `generate-v2.ts`): a
//! provider model with a `base_model` is that lab model (less its identity and links) with the
//! provider file deep-merged over it (tables key by key, arrays and values replaced), less each
//! `base_model_omit` path; its modalities are the provider file's when it has them; its APIs are
//! its own `[api.*]` tables, else the provider's default API, the protocol from the SDK package and
//! the address from the table, the provider, or the SDK's default.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    reason = "a build script fails the build: its panic is the error message"
)]

use std::fs;
use std::path::Path;

use serde_json::{Map, Value, json};

/// The catalog's types, the one place that says which fields llx keeps.
#[path = "src/types.rs"]
#[expect(
    dead_code,
    unreachable_pub,
    reason = "the build reads the types, not their methods"
)]
mod types;

/// models.dev `DEFAULT_NPM_BASE_URLS`, for the packages the copied providers use.
const DEFAULT_BASE_URLS: &[(&str, &str)] = &[
    ("@ai-sdk/openai", "https://api.openai.com/v1"),
    ("@ai-sdk/anthropic", "https://api.anthropic.com/v1"),
    (
        "@ai-sdk/google",
        "https://generativelanguage.googleapis.com/v1beta",
    ),
    ("@ai-sdk/mistral", "https://api.mistral.ai/v1"),
    ("@ai-sdk/cerebras", "https://api.cerebras.ai/v1"),
    ("@ai-sdk/xai", "https://api.x.ai/v1"),
    ("@ai-sdk/groq", "https://api.groq.com/openai/v1"),
    ("@ai-sdk/deepinfra", "https://api.deepinfra.com/v1/openai"),
    ("@ai-sdk/togetherai", "https://api.together.xyz/v1"),
    ("@ai-sdk/perplexity", "https://api.perplexity.ai"),
];

/// What a lab model gives a provider model: not its identity or links.
const NOT_INHERITED: &[&str] = &["id", "benchmarks", "license", "links", "weights"];

fn main() {
    println!("cargo:rerun-if-changed=data");
    // The types cut the files: a field they gain or lose changes the built-in catalog.
    println!("cargo:rerun-if-changed=src/types.rs");
    let data = Path::new("data");
    let mut providers = Map::new();
    for (id, path) in entries(&data.join("providers")) {
        let provider = read(&path.join("provider.toml"));
        let mut models = Map::new();
        let folder = path.join("models");
        for (model_id, file) in files(&folder, &folder) {
            let model = resolve(read(&file), &provider, &data.join("models"));
            // Cut by the types llx reads it with: one that does not read as a model is left out,
            // as a run would leave it out.
            if let Ok(model) = serde_json::from_value::<types::Model>(model) {
                models.insert(model_id, serde_json::to_value(model).unwrap());
            }
        }
        let field = |key: &str| provider.get(key).cloned().unwrap_or(Value::Null);
        providers.insert(
            id.clone(),
            json!({"id": id, "name": field("name"), "env": field("env"), "models": models}),
        );
    }
    // Deflated, the JSON takes a fifteenth of its size in the binary, inflated only by a run that
    // reads it.
    let document = Value::Object(providers).to_string();
    let packed = miniz_oxide::deflate::compress_to_vec(document.as_bytes(), 10);
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("catalog.json.deflate");
    fs::write(out, packed).unwrap();
}

/// A provider model file, resolved.
fn resolve(mut own: Map<String, Value>, provider: &Map<String, Value>, labs: &Path) -> Value {
    own.remove("schema");
    let base = own.remove("base_model");
    let omit = own.remove("base_model_omit");
    let mut model = match base.as_ref().and_then(Value::as_str) {
        Some(base) => {
            let path = labs.join(format!("{base}.toml"));
            let mut lab = read(&path);
            lab.remove("schema");
            for key in NOT_INHERITED {
                lab.remove(*key);
            }
            let mut model = Value::Object(lab);
            merge(&mut model, &Value::Object(own.clone()));
            model
        }
        None => Value::Object(own.clone()),
    };
    for path in omit
        .iter()
        .flat_map(|o| o.as_array().cloned().unwrap_or_default())
    {
        remove(&mut model, path.as_str().unwrap_or(""));
    }
    if let Some(modalities) = own.get("modalities") {
        model["modalities"] = modalities.clone();
    }
    if model.get("type").is_none() {
        model["type"] = json!("chat");
    }
    let merged = model.get("api").cloned().unwrap_or(Value::Null);
    model["api"] = apis(own.get("api"), provider);
    carry_freeform(&mut model, &merged);
    model
}

/// The free-text tools each API takes, from the lab's and the provider's `[api.*]` tables merged:
/// the API's own list, over the model's.
fn carry_freeform(model: &mut Value, merged: &Value) {
    let Some(apis) = model["api"].as_object_mut() else {
        return;
    };
    for (protocol, api) in apis {
        if let Some(kinds) = merged.pointer(&format!("/{protocol}/capabilities/tools/freeform")) {
            api["capabilities"] = json!({"tools": {"freeform": kinds}});
        }
    }
}

/// models.dev's merge: tables key by key, everything else replaced.
fn merge(base: &mut Value, over: &Value) {
    match (base, over) {
        (Value::Object(base), Value::Object(over)) => {
            for (key, value) in over {
                match base.get_mut(key) {
                    Some(slot @ Value::Object(_)) if value.is_object() => merge(slot, value),
                    _ => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, over) => *base = over.clone(),
    }
}

/// Drop a dotted path (`limit.input`).
fn remove(model: &mut Value, path: &str) {
    let mut parts: Vec<&str> = path.split('.').collect();
    let Some(last) = parts.pop() else {
        return;
    };
    let mut at = model;
    for part in parts {
        match at.get_mut(part) {
            Some(next) => at = next,
            None => return,
        }
    }
    if let Some(map) = at.as_object_mut() {
        map.remove(last);
    }
}

/// A model's APIs: its own tables, or the provider's default API.
fn apis(tables: Option<&Value>, provider: &Map<String, Value>) -> Value {
    let npm = provider.get("npm").and_then(Value::as_str).unwrap_or("");
    let address = |table: &Value, sdk: &str| {
        let url = table
            .get("base_url")
            .and_then(Value::as_str)
            .or_else(|| provider.get("api").and_then(Value::as_str))
            .or_else(|| {
                DEFAULT_BASE_URLS
                    .iter()
                    .find(|(p, _)| *p == sdk)
                    .map(|(_, u)| *u)
            })
            .unwrap_or("");
        url.trim_end_matches('/').to_string()
    };
    let mut out = Map::new();
    match tables.and_then(Value::as_object) {
        Some(tables) => {
            for (protocol, table) in tables {
                let sdk = table.pointer("/sdk/aisdk").and_then(Value::as_str);
                let sdk = sdk.filter(|s| *s != "none").unwrap_or(npm);
                out.insert(protocol.clone(), json!({"base_url": address(table, sdk)}));
            }
        }
        None => {
            out.insert(
                protocol(npm).into(),
                json!({"base_url": address(&Value::Null, npm)}),
            );
        }
    }
    Value::Object(out)
}

/// models.dev `resolveProtocol`, for a chat model.
fn protocol(npm: &str) -> &'static str {
    match npm {
        "@ai-sdk/anthropic" => "messages",
        "@ai-sdk/openai" | "@ai-sdk/azure" | "@ai-sdk/xai" => "responses",
        "@ai-sdk/google" => "generate-content",
        _ => "chat-completions",
    }
}

/// The folders under `dir`, by name.
fn entries(dir: &Path) -> Vec<(String, std::path::PathBuf)> {
    let mut out: Vec<_> = fs::read_dir(dir)
        .expect("data/providers")
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().to_string(),
                entry.path(),
            )
        })
        .collect();
    out.sort();
    out
}

/// Every TOML file under `dir`, its id the path from `base` without `.toml`.
fn files(base: &Path, dir: &Path) -> Vec<(String, std::path::PathBuf)> {
    let mut out = vec![];
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files(base, &path));
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            let id = path.strip_prefix(base).unwrap().with_extension("");
            out.push((id.to_string_lossy().into_owned(), path));
        }
    }
    out.sort();
    out
}

fn read(path: &Path) -> Map<String, Value> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let table: toml::Table =
        toml::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let Value::Object(map) = serde_json::to_value(table).unwrap() else {
        unreachable!("a TOML document is a table")
    };
    map
}
