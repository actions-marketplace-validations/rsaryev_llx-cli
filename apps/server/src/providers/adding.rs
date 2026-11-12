//! A host added from the session (`/login`'s "Another host"): checked that it answers, then
//! written into `settings.json` beside the hosts the person named there by hand, under `local`
//! when its address is this machine's and under `hosts` when it is any other.

use local::{Custom, Kind, Known};
use serde_json::{Value, json};

use super::{Provider, catalog, hosts};

/// Whether `address` is on the person's own machine.
fn here(address: &str) -> bool {
    let host = address.split_once("://").map_or(address, |(_, rest)| rest);
    ["localhost", "127.", "[::1]", "0.0.0.0"]
        .iter()
        .any(|own| host.starts_with(own))
}

impl Provider {
    /// The id a host called `name` would have; `Err` says why it cannot be called so.
    pub(crate) fn host_id(name: &str) -> Result<String, String> {
        let id = name.trim().to_ascii_lowercase();
        let plain = |c: char| c.is_ascii_alphanumeric() || "-_.".contains(c);
        if id.is_empty() || !id.chars().all(plain) {
            return Err(format!("{name:?} is not a name (letters, digits, - _ .)"));
        }
        if catalog().callable(&id) {
            return Err(format!("{name} is a provider's own name"));
        }
        Ok(id)
    }

    /// The host at `address` called `name`, with `key` when it asks for one, as a provider:
    /// saved in the settings once it answered. `Err` says why not, in words for the person.
    pub(crate) fn add_host(name: &str, address: &str, key: &str) -> Result<Self, String> {
        let (name, address) = (name.trim(), address.trim());
        let id = Self::host_id(name)?;
        let kind = if here(address) {
            Kind::Local
        } else {
            Kind::Any
        };
        // A bare address: a server on this machine speaks plain HTTP, any other TLS.
        let url = match (address.contains("://"), kind) {
            (true, _) => address.to_string(),
            (false, Kind::Local) => format!("http://{address}"),
            (false, Kind::Any) => format!("https://{address}"),
        };
        let custom = Custom {
            name: name.to_string(),
            kind,
            url: url.clone(),
            key: key.to_string(),
            ..Custom::default()
        };
        let found = local::hosts(None, Known::Skipped, &[custom]).pop();
        let host = found.ok_or_else(|| format!("{address:?} is not an address"))?;
        if hosts::running(&host).is_none() {
            let base = &host.base;
            return Err(format!("{base} does not answer (asked {base}/models)"));
        }
        let path = crate::choice::settings_path().ok_or("no home directory")?;
        let text = std::fs::read_to_string(&path).ok();
        let mut settings = (text.and_then(|text| serde_json::from_str::<Value>(&text).ok()))
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        // One of its name, under either kind, is replaced.
        for section in ["local", "hosts"] {
            // `get_mut`, not an index: an index would write the missing section as `null`.
            if let Some(named) = settings.get_mut(section).and_then(Value::as_array_mut) {
                named.retain(|host| {
                    host["name"]
                        .as_str()
                        .is_none_or(|n| !n.eq_ignore_ascii_case(name))
                });
            }
        }
        let mut entry = json!({"name": name, "url": url});
        if !key.is_empty() {
            entry["key"] = json!(key);
        }
        let section = match kind {
            Kind::Local => "local",
            Kind::Any => "hosts",
        };
        match settings.get_mut(section).and_then(Value::as_array_mut) {
            Some(named) => named.push(entry),
            None => settings[section] = json!([entry]),
        }
        provider_kit::store::save(&path, &format!("{settings:#}\n")).map_err(|e| e.to_string())?;
        Ok(Self(id))
    }
}
