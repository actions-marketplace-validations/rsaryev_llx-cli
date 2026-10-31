//! The catalog's types: a model as models.dev's v2 API publishes it, and what its facts say
//! (usable, its price, its address). `build.rs` includes this file by path to cut the built-in
//! files as a fetched catalog is cut, so it uses std, serde and serde_json alone, nothing of the
//! crate.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A model one provider serves, resolved: the lab's facts with the host's own over them. Only
/// what llx reads is kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model {
    pub name: String,
    /// `chat` unless said.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    /// `alpha`, `beta` or `deprecated`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Modalities>,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<Limit>,
    /// USD per million tokens: `text.input.price`, `text.input.cache.read`, `text.output.price`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Value>,
    /// The APIs it is served through, by protocol, in order of preference.
    #[serde(default)]
    pub api: BTreeMap<String, Api>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Modalities {
    #[serde(default)]
    pub output: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Tools>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tools {
    pub supported: bool,
    /// The kinds of free-text tool it takes: `text`, `lark`, `regex` (the last two hold the text to
    /// a grammar).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub freeform: Vec<String>,
}

/// How this host lets a caller control the model's reasoning.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Reasoning {
    #[serde(default)]
    pub supported: bool,
    /// On and off by a request field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggle: Option<bool>,
    /// Graded levels, low to high (`none` is off).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effort: Vec<String>,
    /// A reasoning-token budget, and its least value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<Budget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<ReasoningDefault>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Budget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
}

/// Where the model starts when no level is asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReasoningDefault {
    #[serde(rename = "type")]
    pub kind: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Limit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Api {
    pub base_url: String,
    /// What the model takes through this API where it differs from the model's own.
    #[serde(default, skip_serializing_if = "ApiCapabilities::is_empty")]
    pub capabilities: ApiCapabilities,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ApiCapabilities {
    #[serde(default)]
    pub tools: ApiTools,
}

impl ApiCapabilities {
    fn is_empty(&self) -> bool {
        self.tools.freeform.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ApiTools {
    /// As [`Tools::freeform`], for this API alone.
    #[serde(default)]
    pub freeform: Vec<String>,
}

impl Model {
    /// A decision model (typed answers, no text), as `llx decide` uses.
    pub fn decides(&self) -> bool {
        self.kind.as_deref() == Some("decision") && self.status.as_deref() != Some("deprecated")
    }

    /// What llx can work with: a chat model that calls tools, writes text, is not deprecated.
    pub fn usable(&self) -> bool {
        self.kind.as_deref().unwrap_or("chat") == "chat"
            && self
                .capabilities
                .tools
                .as_ref()
                .is_some_and(|t| t.supported)
            && self
                .modalities
                .as_ref()
                .is_some_and(|m| m.output.iter().any(|o| o == "text"))
            && self.status.as_deref() != Some("deprecated")
    }

    /// What one request costs, in US dollars, from the catalog's prices:
    /// uncached input at the input price, cached input at the cache-read price (else the
    /// input price), output at the output price; the long-context band by this request's input.
    pub fn price(&self, input: u64, cached: u64, output: u64) -> Option<f64> {
        let text = self.cost.as_ref()?.get("text")?;
        let rates = |side: &str| {
            let base = text.get(side)?;
            // The last band this request's input reaches, else the base prices.
            let band = base
                .get("overrides")
                .and_then(Value::as_array)
                .and_then(|bands| {
                    bands.iter().rev().find(|band| {
                        let at = |key| band.pointer(&format!("/when/context/{key}"))?.as_u64();
                        at("gt").is_some_and(|gt| input > gt)
                            || at("gte").is_some_and(|ge| input >= ge)
                    })
                });
            let price = |key: &str| {
                band.and_then(|b| b.pointer(key))
                    .or_else(|| base.pointer(key))?
                    .as_f64()
            };
            Some((price("/price")?, price("/cache/read")))
        };
        let (input_price, cache_price) = rates("input")?;
        let (output_price, _) = rates("output")?;
        let uncached = input.saturating_sub(cached) as f64;
        let cached = cached as f64 * cache_price.unwrap_or(input_price);
        Some((uncached * input_price + cached + output as f64 * output_price) / 1_000_000.0)
    }

    /// Whether it takes tools of free text held to a Lark grammar through `protocol`: the API's own
    /// list of free-text kinds, else the model's. A catalog that says nothing says no.
    pub fn takes_lark(&self, protocol: &str) -> bool {
        let own = self
            .api
            .get(protocol)
            .map(|api| api.capabilities.tools.freeform.as_slice())
            .filter(|kinds| !kinds.is_empty());
        let kinds = own.unwrap_or_else(|| {
            let tools = self.capabilities.tools.as_ref();
            tools.map_or(&[][..], |tools| tools.freeform.as_slice())
        });
        kinds.iter().any(|kind| kind == "lark")
    }

    /// The address of its API in `protocol`, when it is served so at a fixed address.
    pub fn address(&self, protocol: &str) -> Option<&str> {
        let url = self.api.get(protocol)?.base_url.as_str();
        let fixed =
            (url.starts_with("https://") || url.starts_with("http://")) && !url.contains("${");
        fixed.then_some(url)
    }
}
