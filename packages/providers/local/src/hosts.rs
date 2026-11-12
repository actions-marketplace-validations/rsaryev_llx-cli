//! The hosts looked for: the well-known local servers at the address each listens on by default,
//! then those the person's settings name, local or not.

use crate::{Host, Kind};

/// A host the person's settings name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Custom {
    pub name: String,
    pub kind: Kind,
    /// `http://box:8080/v1`, or the server's address alone (`/v1` is added).
    pub url: String,
    /// The key it asks for; empty for none.
    pub key: String,
    /// Its models, for a host that lists none.
    pub models: Vec<String>,
    /// Its models' context; 0 for unsaid.
    pub context: u64,
    /// The most a reply writes; 0 for unsaid.
    pub output: u64,
}

/// Whether the well-known hosts are looked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Known {
    Looked,
    /// Only the settings' own: a test's, or a machine where the person turned them off.
    Skipped,
}

/// Every host that may run models: Ollama's (at `ollama_host`, as `OLLAMA_HOST` names it, else
/// its default), LM Studio's and llama.cpp's server's, then `custom`. A custom host of a known
/// one's id replaces it.
#[must_use]
pub fn hosts(ollama_host: Option<&str>, known: Known, custom: &[Custom]) -> Vec<Host> {
    let mut hosts: Vec<Host> = match known {
        Known::Skipped => vec![],
        Known::Looked => vec![
            plain("ollama", "Ollama", &ollama(ollama_host)),
            plain("lmstudio", "LM Studio", "http://127.0.0.1:1234/v1"),
            plain("llama.cpp", "llama.cpp", "http://127.0.0.1:8080/v1"),
        ],
    };
    for custom in custom {
        let id = custom.name.trim().to_ascii_lowercase();
        if id.is_empty() || custom.url.trim().is_empty() {
            continue;
        }
        hosts.retain(|host| host.id != id);
        hosts.push(Host {
            id,
            name: custom.name.trim().to_string(),
            kind: custom.kind,
            base: base(&custom.url),
            key: custom.key.clone(),
            models: custom.models.clone(),
            context: custom.context,
            output: custom.output,
        });
    }
    hosts
}

fn plain(id: &str, name: &str, base: &str) -> Host {
    Host {
        id: id.into(),
        name: name.into(),
        base: base.into(),
        ..Host::default()
    }
}

/// Where chat completions are at `url`: as written when it names a path, else its `/v1`.
pub(crate) fn base(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    match rest.contains('/') {
        true => url.to_string(),
        false => format!("{url}/v1"),
    }
}

/// Ollama's address from `OLLAMA_HOST` as Ollama reads it: a scheme, a host and a port, each
/// optional; an address it listens on for every interface is reached on this machine.
fn ollama(host: Option<&str>) -> String {
    let host = host.map_or("", str::trim).trim_end_matches('/');
    let (scheme, rest) = host.split_once("://").unwrap_or(("http", host));
    let (name, port) = match rest.rsplit_once(':') {
        Some((name, port)) if port.parse::<u16>().is_ok() => (name, port),
        Some(_) | None => (rest, "11434"),
    };
    let name = match name {
        "" | "0.0.0.0" | "[::]" => "127.0.0.1",
        name => name,
    };
    format!("{scheme}://{name}:{port}/v1")
}

#[cfg(test)]
#[path = "hosts_tests.rs"]
mod tests;
