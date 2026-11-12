//! What a host runs: the models its `/v1/models` lists, and the context each is loaded with, as
//! the host's own API says it: llama.cpp's `/props` (its one model's), LM Studio's
//! `/api/v0/models` and Ollama's `/api/ps` (each model loaded). A host is asked in each of these
//! dialects until one answers, so one that listens where another does by default is still read.
//! A context no one says stays unknown (0): a model not loaded yet has none.

use contracts::catalog::{Effort, ModelEntry};
use serde_json::Value;

use crate::{Get, Host, Kind};

/// The most a reply writes, in tokens: a local model writes tens of tokens a second, so a reply
/// that runs away is minutes of nothing; one the limit cuts off is gone on from.
const REPLY: u64 = 8_192;

/// A context a dialect names: a model's, or every model's (`None`) where the host runs one.
type Named = (Option<String>, u64);

/// The chat models `host` runs, in its order; `None` when it does not answer. Those the settings
/// list are taken as said, the host not asked.
pub fn models(host: &Host, http: &dyn Get) -> Option<Vec<ModelEntry>> {
    let ids: Vec<String> = match host.models.is_empty() {
        false => host.models.clone(),
        true => {
            let listed = asked(host, http, &format!("{}/models", host.base))?;
            let ids = listed.get("data")?.as_array()?.iter();
            let ids = ids.filter_map(|model| model["id"].as_str());
            ids.filter(|id| !id.to_ascii_lowercase().contains("embed"))
                .map(str::to_string)
                .collect()
        }
    };
    // What a local host loaded each model with, unless the settings say it.
    let named = match (host.kind, host.context) {
        (Kind::Local, 0) => llama(host, http)
            .or_else(|| lm_studio(host, http))
            .or_else(|| ollama(host, http)),
        (Kind::Local | Kind::Any, _) => None,
    };
    let context = |id: &str| {
        let named = named.iter().flatten();
        let said = named
            .filter(|(model, _)| model.as_deref().is_none_or(|m| m == id))
            .map(|(_, context)| *context)
            .next();
        said.unwrap_or(host.context)
    };
    Some(
        (ids.into_iter())
            .map(|id| entry(host, &id, context(&id)))
            .collect(),
    )
}

/// Model `id` of `host` with `context` (0: unknown): no levels, and the most a reply writes as
/// the settings say it, else the kind's own.
pub fn entry(host: &Host, id: &str, context: u64) -> ModelEntry {
    let output = match (host.output, host.kind, context) {
        (0, Kind::Any, _) => 0,
        (0, Kind::Local, 0) => REPLY,
        // A quarter of a small context: the rest is the conversation's.
        (0, Kind::Local, context) => (context / 4).min(REPLY),
        (own, ..) => own,
    };
    ModelEntry {
        id: id.to_string(),
        name: id.to_string(),
        efforts: vec![],
        default: Effort::Off,
        context,
        output,
        marks: false,
    }
}

fn asked(host: &Host, http: &dyn Get, address: &str) -> Option<Value> {
    serde_json::from_str(&http.get(address, &host.key)?).ok()
}

/// llama.cpp's server: the context its model is loaded with.
fn llama(host: &Host, http: &dyn Get) -> Option<Vec<Named>> {
    let props = asked(host, http, &format!("{}/props", host.root()))?;
    let context = props.pointer("/default_generation_settings/n_ctx")?;
    Some(vec![(None, context.as_u64()?)])
}

/// LM Studio: each loaded model's context.
fn lm_studio(host: &Host, http: &dyn Get) -> Option<Vec<Named>> {
    let listed = asked(host, http, &format!("{}/api/v0/models", host.root()))?;
    let models = listed.get("data")?.as_array()?.iter();
    Some(
        models
            .filter_map(|model| {
                let context = model["loaded_context_length"].as_u64()?;
                Some((Some(model["id"].as_str()?.to_string()), context))
            })
            .collect(),
    )
}

/// Ollama: each running model's context.
fn ollama(host: &Host, http: &dyn Get) -> Option<Vec<Named>> {
    let running = asked(host, http, &format!("{}/api/ps", host.root()))?;
    let models = running.get("models")?.as_array()?.iter();
    Some(
        models
            .filter_map(|model| {
                let context = model["context_length"].as_u64()?;
                let id = model["name"].as_str().or_else(|| model["model"].as_str())?;
                Some((Some(id.to_string()), context))
            })
            .collect(),
    )
}

#[cfg(test)]
#[path = "probe_tests.rs"]
mod tests;
