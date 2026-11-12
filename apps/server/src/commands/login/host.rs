//! `/login`'s "Another host": a host the catalog does not know, added by its address, a name
//! and its key when it asks for one, three lines typed in place; it is asked what it runs before
//! anything is saved.

use crate::picker::Terminal;
use crate::providers::Provider;

pub(super) const ROW: &str = "Another host";
const DETAIL: &str = "any OpenAI-compatible address, or a server on this machine";

pub(super) fn item() -> crate::picker::Item {
    crate::picker::Item::new(ROW, DETAIL, false)
}

/// The name offered for the host at `address`: the part of it that says whose it is
/// (`api.together.xyz`: `together`; `box.lan:8080`: `box`), `host` for a bare number.
pub(super) fn suggested(address: &str) -> String {
    let rest = address.split_once("://").map_or(address, |(_, rest)| rest);
    let host = rest.split(['/', ':']).next().unwrap_or_default();
    let mut labels: Vec<&str> = host.split('.').collect();
    if labels.len() > 1 {
        labels.pop();
    }
    let own = (labels.iter().rev()).find(|label| !matches!(**label, "api" | "www" | ""));
    match own {
        Some(label) if !label.chars().all(|c| c.is_ascii_digit()) => label.to_ascii_lowercase(),
        Some(_) | None => "host".into(),
    }
}

/// The host the person adds, saved; `None` when they cancelled, `Err` why it was not added.
pub(super) fn add(
    terminal: &mut dyn Terminal,
    say: &dyn Fn(&str),
) -> Result<Option<Provider>, String> {
    let open = || false;
    let asked = format!(
        "Login › {ROW} › Address: https://llm.example/v1, or http://localhost:8080 for a server here"
    );
    let Some(address) = (terminal.line_until(&asked, &open)).filter(|a| !a.trim().is_empty())
    else {
        return Ok(None);
    };
    // A name the catalog has is not offered, and one typed is asked again, before the key.
    let offered = match suggested(&address) {
        taken if Provider::host_id(&taken).is_err() => format!("my-{taken}"),
        free => free,
    };
    let mut asked = format!("Login › {ROW} › Name: what to call it (Enter for {offered})");
    let name = loop {
        let Some(typed) = terminal.line_until(&asked, &open) else {
            return Ok(None);
        };
        let name = match typed.trim() {
            "" => offered.clone(),
            typed => typed.to_string(),
        };
        match Provider::host_id(&name) {
            Ok(_) => break name,
            Err(why) => {
                asked = format!("Login › {ROW} › Name: {why}; another (Enter for {offered})");
            }
        }
    };
    let asked = format!(
        "Login › {name} › API key: paste it and press Enter (not shown; Enter alone for none)"
    );
    let Some(key) = terminal.secret(&asked) else {
        return Ok(None);
    };
    let provider = Provider::add_host(&name, &address, key.trim())?;
    say(&format!("Added {name}; saved in settings.json.\n"));
    Ok(Some(provider))
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
