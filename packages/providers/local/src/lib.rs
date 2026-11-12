//! The model servers the catalog does not know, of two kinds. Those on the person's own machine
//! ([`Kind::Local`]): Ollama, LM Studio, llama.cpp's server where each listens by default, and
//! any the settings name as local. And any other host that speaks OpenAI's chat completions
//! ([`Kind::Any`]), wherever it is, which only the settings name. No list of their models is
//! kept: a host says what it runs when it is asked ([`models`]), a local one with the context
//! each model is loaded with where it tells it. Whoever composes the providers calls them over
//! chat completions at [`Host::base`].

use std::time::Duration;

use provider_kit::App;
use provider_kit::http::Web;

mod hosts;
mod probe;

pub use hosts::{Custom, Known, hosts};
pub use probe::{entry, models};

/// What a host is, which decides how it is asked and called.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Kind {
    /// A server on the person's own machine: one request at a time, tens of tokens a second, a
    /// context it was loaded with.
    #[default]
    Local,
    /// Any host of chat completions, wherever it is: called as the catalog's plain hosts are.
    Any,
}

/// A server that may run models: where it is, and what it is called.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Host {
    /// What a choice names it by: `ollama`, `lmstudio`, `llama.cpp`, or a name the settings give.
    pub id: String,
    /// What a person reads.
    pub name: String,
    pub kind: Kind,
    /// Where its chat completions are: `http://127.0.0.1:11434/v1`.
    pub base: String,
    /// The key it asks for; empty when it asks for none.
    pub key: String,
    /// The models the settings say it has, for a host that lists none; empty: it is asked.
    pub models: Vec<String>,
    /// The context of its models as the settings say it; 0: as the host says, else unknown.
    pub context: u64,
    /// The most a reply writes as the settings say it; 0: the kind's own.
    pub output: u64,
}

impl Host {
    /// Where its other endpoints are: [`Host::base`] without the `/v1` of OpenAI's.
    pub(crate) fn root(&self) -> &str {
        self.base.strip_suffix("/v1").unwrap_or(&self.base)
    }
}

/// How a host is asked what it runs: a GET answered with a body, or nothing.
pub trait Get: Send + Sync {
    /// The body `address` answers a GET with, `key` its bearer when not empty; `None` when it
    /// did not answer 200.
    fn get(&self, address: &str, key: &str) -> Option<String>;
}

/// How long a host on this machine is waited for: it answers in milliseconds, and a list the
/// person is looking at must not hang on one that is gone.
const PATIENCE: Duration = Duration::from_secs(2);
/// And a host anywhere: a TLS handshake away.
const FAR: Duration = Duration::from_secs(5);

/// [`Get`] over the network.
pub struct Asked(Web);

impl Asked {
    #[must_use]
    pub fn new(app: App, kind: Kind) -> Self {
        Self(Web::new(
            app,
            match kind {
                Kind::Local => PATIENCE,
                Kind::Any => FAR,
            },
        ))
    }
}

impl Get for Asked {
    fn get(&self, address: &str, key: &str) -> Option<String> {
        let bearer = format!("Bearer {key}");
        let headers: &[(&str, &str)] = match key.is_empty() {
            true => &[],
            false => &[("Authorization", &bearer)],
        };
        let reply = self.0.get(address, headers).ok()?;
        (reply.status == 200).then_some(reply.body)
    }
}
