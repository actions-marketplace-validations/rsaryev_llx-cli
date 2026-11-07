//! What a host asks to have sent back with a turn, to the model that wrote it alone: its
//! reasoning (OpenRouter's `reasoning_details`, signed or sealed, else its text in the field it
//! came in), an answer it gave in chunks (Mistral's, its thinking among them) as it came, and
//! each call's own extra (Gemini's thought signature, refused with a 400 when it is missing).

use serde_json::{Map, Value, json};

use crate::chat::THOUGHT;

/// What goes back with the turn `message` is, from `model`: `None` when nothing does.
pub(crate) fn carried(message: &Value, model: &str) -> Option<Value> {
    let mut fields = Map::new();
    if let Some(details) = message
        .get("reasoning_details")
        .filter(|details| details.as_array().is_some_and(|parts| !parts.is_empty()))
    {
        fields.insert("reasoning_details".into(), details.clone());
    } else if let Some((field, text)) = THOUGHT.iter().find_map(|field| {
        let text = message
            .get(*field)?
            .as_str()
            .filter(|text| !text.is_empty())?;
        Some((*field, text))
    }) {
        fields.insert(field.into(), json!(text));
    }
    if let Some(chunks) = message.get("content").filter(|content| content.is_array()) {
        fields.insert("content".into(), chunks.clone());
    }
    let extras: Map<String, Value> = (message["tool_calls"].as_array().into_iter().flatten())
        .filter_map(|call| {
            Some((
                call["id"].as_str()?.to_string(),
                call.get("extra_content")?.clone(),
            ))
        })
        .collect();
    if fields.is_empty() && extras.is_empty() {
        return None;
    }
    let mut kept = json!({"model": model, "fields": fields});
    if !extras.is_empty() {
        kept["calls"] = Value::Object(extras);
    }
    Some(kept)
}

/// `out`, an assistant message for `model`, with what its turn carried put back: only what this
/// provider kept for that model (another's, a Responses reasoning, has no `fields`).
pub(crate) fn restore(out: &mut Value, carried: &[Value], model: &str) {
    for kept in carried.iter().filter(|kept| kept["model"] == model) {
        for (field, value) in kept["fields"].as_object().into_iter().flatten() {
            out[field.as_str()] = value.clone();
        }
        for (id, extra) in kept["calls"].as_object().into_iter().flatten() {
            let calls = out["tool_calls"].as_array_mut().into_iter().flatten();
            for call in calls.filter(|call| call["id"] == id.as_str()) {
                call["extra_content"] = extra.clone();
            }
        }
    }
}

/// The words of an answer given in chunks: its text chunks, joined.
pub(crate) fn words(content: &Value) -> Option<String> {
    let chunks = content.as_array()?;
    let texts = chunks.iter().filter(|chunk| chunk["type"] == "text");
    Some(texts.filter_map(|chunk| chunk["text"].as_str()).collect())
}

#[cfg(test)]
#[path = "carry_tests.rs"]
mod tests;
