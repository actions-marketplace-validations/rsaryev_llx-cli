//! The model catalog: what llx knows of models and their providers, as models.dev knows it (see
//! MODELS.md), and in one place only. The base is models.dev's own files in `data/`, resolved at
//! build time into the shape of its v2 API; models.dev's published v2 API, fetched in the
//! background ([`source`]), goes over it. A model's window, reasoning levels, default level,
//! price and API address all come from here. Reading a catalog decodes none of its models: each
//! is decoded when asked for (`Provider`).

mod provider;
pub mod source;
mod types;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use contracts::catalog::{Effort, ModelEntry};
use serde_json::Value;

use provider::Provider;
use types::{Model, Reasoning};

/// The providers listed first, in this order; the rest follow by id.
pub const POPULAR: [&str; 18] = [
    "openai",
    "anthropic",
    "google",
    "openrouter",
    "xai",
    "deepseek",
    "mistral",
    "groq",
    "moonshotai",
    "zai",
    "alibaba",
    "togetherai",
    "fireworks-ai",
    "cerebras",
    "nvidia",
    "ollama-cloud",
    "perplexity",
    "deepinfra",
];

/// The protocol llx speaks to any host.
pub const CHAT_COMPLETIONS: &str = "chat-completions";
/// The protocol of OpenAI's own backend, which some of its models are served over alone.
pub const RESPONSES: &str = "responses";

/// Every provider llx knows, by id.
#[derive(Debug, Clone)]
pub struct Catalog {
    providers: BTreeMap<String, Provider>,
    /// The built-in files it lies over, read the first time a provider it lacks is asked for.
    under: Option<OnceLock<Box<Catalog>>>,
}

impl Catalog {
    /// A document in the shape of models.dev's v2 API: read whole (one cut short is refused), its
    /// models decoded as they are asked for.
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let providers = serde_json::from_str(text).map_err(|error| error.to_string())?;
        Ok(Self {
            providers,
            under: None,
        })
    }

    /// models.dev's files built in, inflated.
    #[expect(
        clippy::expect_used,
        reason = "build.rs wrote this document in this shape, and the tests parse it"
    )]
    pub fn built_in() -> Self {
        let packed = include_bytes!(concat!(env!("OUT_DIR"), "/catalog.json.deflate"));
        let inflated = miniz_oxide::inflate::decompress_to_vec(packed);
        let text = (inflated.map_err(|error| error.to_string()))
            .and_then(|bytes| String::from_utf8(bytes).map_err(|error| error.to_string()));
        text.and_then(|text| Self::parse(&text))
            .expect("the built-in catalog is valid")
    }

    /// This catalog over the built-in files: a provider it lacks is the built-in one. They are
    /// read only when such a provider is asked for, or the list of them.
    #[must_use]
    pub(crate) fn over_built_in(self) -> Self {
        let under = Some(OnceLock::new());
        Self { under, ..self }
    }

    fn under(&self) -> Option<&Catalog> {
        let read = self.under.as_ref()?;
        Some(read.get_or_init(|| Box::new(Self::built_in())))
    }

    /// The providers llx can call (a usable model over chat completions), the popular first.
    pub fn ids(&self) -> Vec<&str> {
        let under = self
            .under()
            .into_iter()
            .flat_map(|under| under.providers.keys());
        let mut ids: Vec<&str> = (self.providers.keys().chain(under))
            .map(String::as_str)
            .collect();
        let rank = |id: &str| {
            POPULAR
                .iter()
                .position(|p| *p == id)
                .unwrap_or(POPULAR.len())
        };
        ids.sort_unstable_by_key(|id| (rank(id), *id));
        ids.dedup();
        ids.retain(|id| self.callable(id));
        ids
    }

    /// Whether llx can call `id`, one of [`Catalog::ids`]: its models decoded up to the first
    /// usable one.
    pub fn callable(&self, id: &str) -> bool {
        let found = self.provider(id);
        found.is_some_and(|p| p.models().any(|(_, m)| calls(m, CHAT_COMPLETIONS)))
    }

    /// Provider `id`: this catalog's, else the built-in one.
    pub fn provider(&self, id: &str) -> Option<&Provider> {
        (self.providers.get(id)).or_else(|| self.under()?.provider(id))
    }

    /// `provider`'s model `id`, when it has one that reads as a model.
    pub fn model(&self, provider: &str, id: &str) -> Option<&Model> {
        self.provider(provider)?.model(id)
    }

    /// Whether `provider`'s model `id` takes tools of free text held to a Lark grammar over
    /// `protocol`. models.dev's v2 API does not publish it: a fetched catalog asks the built-in
    /// files too, which do.
    pub fn takes_lark(&self, provider: &str, id: &str, protocol: &str) -> bool {
        let says = |catalog: &Self| {
            (catalog.model(provider, id)).is_some_and(|model| model.takes_lark(protocol))
        };
        says(self) || self.under().is_some_and(says)
    }

    /// `provider`'s models llx can call over `protocol`, newest first.
    pub fn served(&self, provider: &str, protocol: &str) -> Vec<ModelEntry> {
        let Some(found) = self.provider(provider) else {
            return vec![];
        };
        let mut models: Vec<(&str, &Model)> =
            found.models().filter(|(_, m)| calls(m, protocol)).collect();
        models.sort_by(|a, b| b.1.release_date.cmp(&a.1.release_date));
        models
            .into_iter()
            .map(|(id, model)| entry(id, model))
            .collect()
    }

    /// Whether `provider`'s model `id` is one of those it serves llx over `protocol`
    /// ([`Catalog::served`]), with no other model decoded.
    pub fn serves(&self, provider: &str, id: &str, protocol: &str) -> bool {
        self.model(provider, id).is_some_and(|m| calls(m, protocol))
    }

    /// The entry for `provider`'s model `id`, when the catalog has it.
    pub fn entry(&self, provider: &str, id: &str) -> Option<ModelEntry> {
        self.model(provider, id).map(|model| entry(id, model))
    }

    /// The catalog's own providers as a document to keep, cut down to those llx can call, their
    /// chat models and the decision models.
    pub(crate) fn kept(&self) -> String {
        let keep = |model: &Model| model.usable() || model.decides();
        let kept: BTreeMap<&String, provider::Kept> = (self.providers.iter())
            .filter_map(|(id, provider)| Some((id, provider.kept(keep)?)))
            .collect();
        serde_json::to_string(&kept).unwrap_or_default()
    }
}

/// Whether llx can call `model` over `protocol`: usable, at a fixed address.
fn calls(model: &Model, protocol: &str) -> bool {
    model.usable() && model.address(protocol).is_some()
}

fn entry(id: &str, model: &Model) -> ModelEntry {
    let reasoning = model.capabilities.reasoning.clone().unwrap_or_default();
    let efforts = efforts(&reasoning);
    let limit = model.limit.as_ref();
    ModelEntry {
        id: id.to_string(),
        name: model.name.clone(),
        default: default(&reasoning, &efforts),
        efforts,
        context: limit.and_then(|l| l.input.or(l.context)).unwrap_or(0),
        output: limit.and_then(|l| l.output).unwrap_or(0),
        marks: takes_marks(id),
    }
}

/// Whether model `id` caches a prompt only where told to: Anthropic, Gemini and Qwen take
/// `cache_control` breakpoints (OpenRouter's prompt-caching guide); OpenAI, DeepSeek, Grok,
/// Moonshot and the rest cache a repeated start on their own. The catalog's `cache.explicit` also
/// covers other mechanisms (OpenAI's own breakpoint field), so it cannot say this; ids name the
/// vendor first, and that is what is read, here beside the data, not in a provider (A4).
pub fn takes_marks(id: &str) -> bool {
    ["anthropic/", "google/gemini", "qwen/"]
        .iter()
        .any(|vendor| id.starts_with(vendor))
}

/// The levels a host takes for a model, lowest first (MODELS.md, reasoning controls).
fn efforts(reasoning: &Reasoning) -> Vec<Effort> {
    if !reasoning.supported {
        return vec![];
    }
    let mut levels: Vec<Effort> = reasoning
        .effort
        .iter()
        .filter_map(|name| Effort::named(name))
        .collect();
    if reasoning.toggle == Some(true) {
        levels.push(Effort::Off);
        if reasoning.effort.is_empty() {
            levels.push(Effort::Medium);
        }
    }
    if let Some(budget) = &reasoning.budget {
        levels.extend([Effort::Low, Effort::Medium, Effort::High]);
        if budget.min.unwrap_or(0) <= 0 {
            levels.push(Effort::Off);
        }
    }
    levels.sort();
    levels.dedup();
    levels
}

/// Where a model starts: the host's own default, else medium or the nearest below, else the
/// lowest level it takes.
fn default(reasoning: &Reasoning, efforts: &[Effort]) -> Effort {
    let stated = reasoning
        .default
        .as_ref()
        .and_then(|d| match (d.kind.as_str(), &d.value) {
            ("effort", Value::String(name)) => Effort::named(name),
            ("toggle", Value::Bool(false)) => Some(Effort::Off),
            _ => None,
        });
    if let Some(stated) = stated.filter(|level| efforts.contains(level)) {
        return stated;
    }
    let below = efforts
        .iter()
        .rev()
        .find(|e| **e <= Effort::Medium && **e != Effort::Off);
    below
        .or_else(|| efforts.first())
        .copied()
        .unwrap_or(Effort::Off)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
