//! The providers llx can think with: every one the model catalog lists (`models`, from
//! models.dev), and the one place that knows the provider packages: how each is signed in to,
//! where its key is kept, and how its chat model is built. OpenAI is the ChatGPT backend with a
//! ChatGPT sign-in, else its API with a key; Anthropic's own models go over its Messages API;
//! every other is called over chat completions at the address the catalog gives (OpenRouter with
//! its own extras).

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use contracts::catalog::{Effort, ModelEntry};
use models::{CHAT_COMPLETIONS, Catalog, RESPONSES};

use crate::compose::APP;

mod chat;
mod deciding;
mod kept;
mod login;

pub(crate) use deciding::{adviser, decider};

/// Where a key's variables are read: the environment the workspace gives (a test's), else, for
/// `None`, this process's.
pub(crate) type Vars<'a> = Option<&'a [(String, String)]>;

/// Variable `name` in `vars`.
fn var(vars: Vars, name: &str) -> Option<String> {
    match vars {
        Some(vars) => vars
            .iter()
            .find(|(set, _)| set == name)
            .map(|(_, v)| v.clone()),
        None => std::env::var(name).ok(),
    }
}

/// A provider, by its catalog id (`openai`, `openrouter`, `deepseek`…).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Provider(pub String);

/// How a provider is signed in to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Method {
    /// OAuth in the system browser, back to a local callback.
    Browser,
    /// A key the person pastes.
    Key,
}

/// The model a provider starts with, where llx has a view: a policy, by catalog id.
const DEFAULTS: [(&str, &str); 2] = [
    ("openai", "gpt-6-luna"),
    ("openrouter", "anthropic/claude-haiku-5.5"),
];

/// The decision model `llx decide` and `llx doctor` start with, and the one a session's tasks
/// are advised by: a policy, by catalog id (OpenRouter's).
pub(crate) const DECISION_DEFAULT: &str = "~typesafe/jev-latest";

/// The providers whose models are called over the Messages API: those its wire is built for.
const MESSAGES_HOSTS: [&str; 1] = ["anthropic"];

/// How often a long-lived process looks for a newer catalog: a file's time, and the refresh
/// asked, which asks models.dev at most once an hour. A `/model` opened a minute after a
/// refresh wrote one lists its models.
const LOOK: Duration = Duration::from_secs(60);

/// The catalog: the built-in files under the last fetched one, read again when a refresh wrote
/// a newer one (unit tests see the built-in files only, the same on every machine).
pub(crate) fn catalog() -> Arc<Catalog> {
    if cfg!(test) {
        static BUILT_IN: OnceLock<Arc<Catalog>> = OnceLock::new();
        return Arc::clone(BUILT_IN.get_or_init(|| Arc::new(Catalog::built_in())));
    }
    static KEPT: kept::Kept = kept::Kept::new(LOOK);
    let written = || models::source::written(APP);
    let (catalog, ask) = KEPT.get(written, || models::source::load(APP));
    if ask {
        refresh_catalog();
    }
    catalog
}

impl Provider {
    pub(crate) fn openai() -> Self {
        Self("openai".into())
    }

    pub(crate) fn openrouter() -> Self {
        Self("openrouter".into())
    }

    /// Every provider llx can call, the popular first.
    pub(crate) fn all() -> Vec<Provider> {
        catalog()
            .ids()
            .into_iter()
            .map(|id| Self(id.into()))
            .collect()
    }

    /// Where it stands among the popular providers, listed first; `None` when it is not one.
    pub(crate) fn popular(&self) -> Option<usize> {
        models::POPULAR.iter().position(|id| *id == self.0)
    }

    /// The provider `id` names, when the catalog lists it (and no other is looked at).
    pub(crate) fn named(id: &str) -> Option<Self> {
        let id = id.trim().to_ascii_lowercase();
        catalog().callable(&id).then_some(Self(id))
    }

    pub(crate) fn id(&self) -> &str {
        &self.0
    }

    /// What the person reads.
    pub(crate) fn name(&self) -> String {
        catalog()
            .provider(&self.0)
            .map_or_else(|| self.0.clone(), |p| p.name().to_string())
    }

    /// How it is signed in to, where that is not just a key: said where the provider is picked.
    pub(crate) fn hint(&self) -> Option<&'static str> {
        (self.0 == "openai").then_some("ChatGPT sign-in, or an API key")
    }

    pub(crate) fn methods(&self) -> &'static [Method] {
        match self.0.as_str() {
            "openai" => &[Method::Browser, Method::Key],
            "openrouter" => &[Method::Key, Method::Browser],
            _ => &[Method::Key],
        }
    }

    /// The models it is called with: OpenAI's over the Responses API (the ChatGPT backend with a
    /// sign-in, the API with a key), any other provider's over chat completions.
    pub(crate) fn models(&self) -> Vec<ModelEntry> {
        catalog().served(&self.0, self.protocol())
    }

    fn protocol(&self) -> &'static str {
        match self.0 == "openai" {
            true => RESPONSES,
            false => CHAT_COMPLETIONS,
        }
    }

    /// The entry for model `id`; one the catalog does not know is offered no level.
    pub(crate) fn entry(&self, id: &str) -> ModelEntry {
        catalog().entry(&self.0, id).unwrap_or_else(|| ModelEntry {
            id: id.to_string(),
            name: id.to_string(),
            efforts: vec![],
            default: Effort::Off,
            context: 0,
            output: 0,
            marks: models::takes_marks(id),
        })
    }

    /// llx's default where it has one and the catalog lists it, else the newest model.
    pub(crate) fn default_model(&self) -> String {
        let protocol = self.protocol();
        let stated = DEFAULTS.iter().find(|(p, _)| *p == self.0).map(|(_, m)| *m);
        // The stated model alone is decoded; every one only when it is not served, for the newest.
        if let Some(id) = stated.filter(|id| catalog().serves(&self.0, id, protocol)) {
            return id.to_string();
        }
        let models = catalog().served(&self.0, protocol);
        models.first().map_or_else(String::new, |m| m.id.clone())
    }

    /// Where the sign-in comes from, in words with no secret in them; `None` when signed out.
    pub(crate) fn signed_in(&self) -> Option<String> {
        self.signed_in_by(None)
    }

    /// [`Provider::signed_in`], its key's variables read in `vars`.
    pub(crate) fn signed_in_by(&self, vars: Vars) -> Option<String> {
        if self.0 == "openai"
            && let Some(tokens) = openai::tokens::load(APP)
        {
            return Some(format!("ChatGPT account {}", tokens.account));
        }
        self.key_in(vars).map(|(_, from)| from)
    }

    /// The key and where it comes from: its variable (as the catalog names them) in `vars`, else
    /// the one saved for llx.
    pub(crate) fn key_in(&self, vars: Vars) -> Option<(String, String)> {
        key_from(&self.variables(), vars, self.key_path())
    }

    /// What a run that is not signed in says: a session's /login, or the variable its key is
    /// read from.
    pub(crate) fn how_to_sign_in(&self) -> String {
        sign_in_words(&self.name(), self.variables().first().map(String::as_str))
    }

    /// Whether the key comes from the environment, which no saved one replaces.
    pub(crate) fn key_in_env(&self, vars: Vars) -> bool {
        (self.variables().iter()).any(|name| var(vars, name).is_some_and(|k| !k.trim().is_empty()))
    }

    /// What commands never see: the variables its key is read from, and OpenRouter's, which
    /// the decision model is called with.
    pub(crate) fn hidden(&self) -> Vec<String> {
        let mut names = self.variables();
        names.retain(|name| name != openrouter::auth::KEY_VARIABLE);
        names.push(openrouter::auth::KEY_VARIABLE.to_string());
        names
    }

    fn variables(&self) -> Vec<String> {
        catalog()
            .provider(&self.0)
            .map_or_else(Vec::new, |p| p.env().to_vec())
    }

    /// Where its pasted key is kept (OpenRouter's where it always was).
    pub(crate) fn key_path(&self) -> Option<PathBuf> {
        match self.0.as_str() {
            "openrouter" => openrouter::auth::credentials_path(APP),
            id => provider_kit::store::path(APP, &format!("key-{id}")),
        }
    }

    /// Keep `key` for llx; where.
    pub(crate) fn save_key(&self, key: &str) -> std::io::Result<PathBuf> {
        let path = self
            .key_path()
            .ok_or_else(|| std::io::Error::other("no home directory"))?;
        provider_kit::store::save(&path, key)?;
        Ok(path)
    }

    /// Forget the saved sign-in; whether there was one.
    pub(crate) fn forget(&self) -> std::io::Result<bool> {
        let mut removed = false;
        if self.0 == "openai" {
            removed |= openai::tokens::remove(APP)?;
        }
        if let Some(path) = self.key_path() {
            removed |= provider_kit::store::remove(&path)?;
        }
        Ok(removed)
    }
}

/// OpenRouter's client for `key`, at the address the session's environment names for a server
/// standing in for it, else at its own.
/// What of a value an old turn carried back may go (the history's `Shed`): each provider that
/// takes it knows its own; chat completions' hosts let none go (DeepSeek and Kimi ask for their
/// reasoning back through a task's calls, Gemini for its signatures).
pub(crate) fn shed(kept: &mut serde_json::Value) -> bool {
    openai::responses::shed(kept) || anthropic::chat::shed(kept)
}

pub(crate) fn openrouter_client(key: String, vars: Vars) -> openrouter::client::OpenRouter {
    match var(vars, "OPENROUTER_BASE_URL") {
        Some(base) => openrouter::client::OpenRouter::standing_in(&base, key, APP),
        None => openrouter::client::OpenRouter::new(key, APP),
    }
}

/// What `llx decide` and `llx doctor` say with no key for the decision model, read
/// off no catalog.
pub(crate) fn how_to_decide() -> String {
    sign_in_words(openrouter::NAME, Some(openrouter::auth::KEY_VARIABLE))
}

/// The first of `names` set in `vars`, trimmed, and its name; else the key saved at `saved`, and
/// where it is.
fn key_from(names: &[String], vars: Vars, saved: Option<PathBuf>) -> Option<(String, String)> {
    let set = names.iter().find_map(|name| {
        let key = var(vars, name).filter(|k| !k.trim().is_empty())?;
        Some((key.trim().to_string(), name.clone()))
    });
    set.or_else(|| {
        let path = saved?;
        let key = provider_kit::store::read(&path)?;
        let path = crate::commands::sessions::home_relative(&path.display().to_string());
        Some((key, format!("the key in {path}")))
    })
}

/// Not signed in to `name`: a session's /login, or the `variable` its key is read from.
fn sign_in_words(name: &str, variable: Option<&str>) -> String {
    let login = format!("not signed in to {name}: open a session (`llx`) and type /login");
    match variable {
        Some(variable) => format!("{login}, or set {variable}"),
        None => login,
    }
}

/// Ask models.dev for a newer catalog, at most once an hour, on a thread of its own: the next
/// session reads what it keeps.
pub(crate) fn refresh_catalog() {
    // `LLX_MODELS_REFRESH=0`: no network for it (tests, offline machines).
    if std::env::var("LLX_MODELS_REFRESH").is_ok_and(|v| v == "0") {
        return;
    }
    let refreshing = std::thread::Builder::new().name("llx-catalog".into());
    // No thread: the catalog kept is read again, as when the network fails.
    let _ = refreshing.spawn(|| {
        let _ = models::source::refresh(APP);
    });
}

#[cfg(test)]
#[path = "providers_tests.rs"]
mod tests;
