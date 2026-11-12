//! The hosts the catalog does not know, as providers: the model servers on the person's own
//! machine (Ollama, LM Studio, llama.cpp's server, and those `settings.json` names under
//! `local`), and any other host of chat completions it names under `hosts` (`local`, the
//! package, puts them together). A host says what it runs when it is asked, and is asked again a
//! few seconds later, so a model loaded meanwhile is listed. A catalog provider of the same id
//! comes first.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use contracts::catalog::ModelEntry;
use local::{Asked, Custom, Host, Kind, Known};
use serde_json::Value;

use super::{Provider, Vars, var};
use crate::compose::APP;

/// How long what a host said is believed: one command (a session's start, `/model`) asks a host
/// several things, and each would be a request of its own.
const BELIEVED: Duration = Duration::from_secs(3);

/// What `/model` says under `Local` when no server on this machine answers.
pub(crate) const NONE_RUNNING: &str = "none running (Ollama, LM Studio, llama-server)";

/// Whether the well-known servers are looked for: `LLX_LOCAL=0` leaves them out (tests, and a
/// machine whose ports are another program's).
fn known(vars: Vars) -> Known {
    match var(vars, "LLX_LOCAL").is_some_and(|v| v == "0") {
        true => Known::Skipped,
        false => Known::Looked,
    }
}

/// The hosts looked for: the well-known ones, then those the settings name, which stay.
pub(crate) fn hosts(vars: Vars) -> Vec<Host> {
    // A unit test looks at no port and no settings of the machine it runs on.
    if cfg!(test) {
        return vec![];
    }
    let ollama = var(vars, "OLLAMA_HOST");
    let settings = crate::choice::settings_path().and_then(|p| std::fs::read_to_string(p).ok());
    local::hosts(ollama.as_deref(), known(vars), &custom(settings.as_deref()))
}

/// The hosts `settings.json` names: servers on this machine under `local`, any other under
/// `hosts`, each `{"name": "box", "url": "http://box:8080"}` with a `key` when it asks for one,
/// and what it does not say itself: its `models`, their `context`, a reply's most (`output`).
fn custom(settings: Option<&str>) -> Vec<Custom> {
    let settings = settings.and_then(|text| serde_json::from_str::<Value>(text).ok());
    let text = |host: &Value, field: &str| host[field].as_str().unwrap_or_default().to_string();
    let named = |key: &str, kind: Kind| {
        let hosts = settings.as_ref().and_then(|s| s.get(key)?.as_array());
        let hosts = hosts.map_or_else(Vec::new, Clone::clone).into_iter();
        hosts.map(move |host| Custom {
            name: text(&host, "name"),
            kind,
            url: text(&host, "url"),
            key: text(&host, "key"),
            models: (host["models"].as_array().into_iter().flatten())
                .filter_map(|model| model.as_str().map(str::to_string))
                .collect(),
            context: host["context"].as_u64().unwrap_or(0),
            output: host["output"].as_u64().unwrap_or(0),
        })
    };
    named("local", Kind::Local)
        .chain(named("hosts", Kind::Any))
        .collect()
}

/// The catalog's provider `host` is under another name: a host anywhere at the address of that
/// provider's API (OpenRouter, Anthropic), called as the provider is, with the host's key, so
/// its models keep their levels, their prices and the prompt cache.
pub(super) fn stands_for(host: &Host) -> Option<Provider> {
    let found =
        (host.kind == Kind::Any).then(|| super::catalog().at(&host.base).map(str::to_string));
    found.flatten().map(Provider)
}

/// What a host at an address said it runs, and when.
type Said = (String, Instant, Option<Vec<ModelEntry>>);

/// The models `host` runs, as it said within [`BELIEVED`], else asked now; `None` when it does
/// not answer.
pub(super) fn running(host: &Host) -> Option<Vec<ModelEntry>> {
    // A catalog provider under another name is not asked: the catalog says what it serves, and
    // its key signs in to it.
    if let Some(provider) = stands_for(host) {
        let named = host.models.iter().map(|id| provider.entry(id)).collect();
        let models = match host.models.is_empty() {
            true => provider.models(),
            false => named,
        };
        return (!host.key.is_empty()).then_some(models);
    }
    static SAID: Mutex<Vec<Said>> = Mutex::new(vec![]);
    let said = {
        let mut said = crate::sync::lock(&SAID);
        said.retain(|(_, at, _)| at.elapsed() < BELIEVED);
        let found = said.iter().find(|(base, ..)| *base == host.base);
        found.map(|(.., models)| models.clone())
    };
    // Asked outside the lock (R6): a host that is gone is waited for.
    said.unwrap_or_else(|| {
        let models = local::models(host, &Asked::new(APP, host.kind)).map(|models| {
            models
                .into_iter()
                .map(|model| cataloged(host, model))
                .collect()
        });
        crate::sync::lock(&SAID).push((host.base.clone(), Instant::now(), models.clone()));
        models
    })
}

/// The entry for model `id` of `host`: as the host says it, else a model it does not list (one
/// it loads when asked), its context what the settings say, else unknown.
pub(super) fn entry(host: &Host, id: &str) -> ModelEntry {
    if let Some(provider) = stands_for(host) {
        return provider.entry(id);
    }
    let listed = running(host).and_then(|models| models.into_iter().find(|m| m.id == id));
    listed.unwrap_or_else(|| cataloged(host, local::entry(host, id, host.context)))
}

/// `model` of a host anywhere as the catalog knows it under any provider (its levels, its
/// limits, its cache marks: a proxy or a router serving a vendor's model), what the host or the
/// settings said of its context and its reply kept; as it is when the catalog has none of its id.
/// A server on this machine runs its own build of a model, with the context it was loaded with.
fn cataloged(host: &Host, model: ModelEntry) -> ModelEntry {
    let found = (host.kind == Kind::Any).then(|| super::catalog().known(&model.id));
    let Some(known) = found.flatten() else {
        return model;
    };
    let own = |said: u64, catalog: u64| if said == 0 { catalog } else { said };
    ModelEntry {
        context: own(model.context, known.context),
        output: own(model.output, known.output),
        ..known
    }
}

impl Provider {
    /// The host this names, where the catalog has no provider of its id.
    pub(crate) fn host(&self, vars: Vars) -> Option<Host> {
        if super::catalog().callable(&self.0) {
            return None;
        }
        hosts(vars).into_iter().find(|host| host.id == self.0)
    }

    /// The hosts that answer now: those anywhere, then the servers on this machine, each in the
    /// order they are looked for.
    pub(crate) fn local(vars: Vars) -> Vec<Provider> {
        let hosts = hosts(vars).into_iter();
        let (any, local): (Vec<Host>, Vec<Host>) = hosts
            .filter(|host| running(host).is_some())
            .partition(|host| host.kind == Kind::Any);
        let found = any.into_iter().chain(local);
        found.map(|host| Self(host.id)).collect()
    }

    /// What `/model` says when no server on this machine answers, where they are looked for.
    pub(crate) fn none_local(vars: Vars, found: &[Provider]) -> Option<&'static str> {
        let looked = !cfg!(test) && known(vars) == Known::Looked;
        let any = found.iter().any(|p| p.section() == "Local");
        (looked && !any).then_some(NONE_RUNNING)
    }

    /// What it is one of, as `/model` names its sections.
    pub(crate) fn section(&self) -> &'static str {
        match self.host(None).map(|host| host.kind) {
            None => "Cloud",
            Some(Kind::Any) => "Hosts",
            Some(Kind::Local) => "Local",
        }
    }

    /// Whether it is a server on the person's own machine: one request at a time, one cache.
    pub(crate) fn on_this_machine(&self, vars: Vars) -> bool {
        self.host(vars).is_some_and(|host| host.kind == Kind::Local)
    }

    /// Every host looked for, with where it is and how many models it runs (`None`: it does
    /// not answer): `llx doctor`'s lines.
    pub(crate) fn local_report() -> Vec<(Provider, String, Option<usize>)> {
        (hosts(None).into_iter())
            .map(|host| {
                let models = running(&host).map(|models| models.len());
                (Self(host.id), host.base, models)
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "hosts_tests.rs"]
mod tests;
