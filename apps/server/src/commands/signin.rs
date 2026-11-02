//! Signing in to a provider, for `/login`, and `llx logout`, which forgets every saved sign-in.
//! Keys and tokens are never shown: a key is read with the echo off, and only its masked ends
//! are printed.

use std::sync::Mutex;
use std::time::Duration;

use provider_kit::{oauth, store};

use crate::error::{self, Failure};
use crate::exit;
use crate::picker::Terminal;
use crate::providers::{Method, Provider, Vars};
use crate::sync::lock;

/// Sign in to `provider` with `method`; what was saved, in words with no secret in them.
pub(super) fn sign_in(
    provider: &Provider,
    method: Method,
    path: &str,
    terminal: &mut dyn Terminal,
    say: &dyn Fn(&str),
    vars: Vars,
) -> Result<Option<String>, String> {
    if let (Method::Browser, Some(login)) = (method, provider.browser_login(vars)) {
        let login = login?;
        say(&format!(
            "Opening the sign-in in your browser:\n  {}\n",
            login.address()
        ));
        oauth::open(login.address());
        let came = come_back(terminal, path, say, &|t| login.wait(t), &|p| {
            login.pasted(p)
        })?;
        return came.map(|code| login.finish(&code)).transpose();
    }
    let prompt = format!("{path} › API key: paste it and press Enter (it is not shown)");
    let Some(line) = terminal.secret(&prompt) else {
        return Ok(None);
    };
    let key = store::as_key(&line).ok_or("that is not a key")?;
    let file = provider.save_key(key).map_err(|e| e.to_string())?;
    let mut said = format!(
        "Saved the key {}; saved in {}.",
        store::masked(key),
        file.display()
    );
    if provider.key_in_env(vars) {
        said.push_str(" Its variable is set, and it wins over the saved key.");
    }
    Ok(Some(said))
}

/// Wait for the browser to come back, or for the address it ended on to be pasted (when it is
/// on another machine); the authorization code. The line for the address closes by itself once
/// the browser is back: nothing to press. It is read on this thread only, so nothing is left
/// reading stdin once the sign-in ends. `None` when the person cancelled.
fn come_back(
    terminal: &mut dyn Terminal,
    path: &str,
    say: &dyn Fn(&str),
    wait: &dyn Fn(Duration) -> Option<Result<String, String>>,
    pasted: &dyn Fn(&str) -> Result<String, String>,
) -> Result<Option<String>, String> {
    let prompt = format!(
        "{path} › Finish signing in in the browser; llx goes on by itself. If the browser is \
         on another machine, paste the address it ended on (Enter checks again, q goes back)"
    );
    let back = Mutex::new(None);
    let done = || {
        let mut back = lock(&back);
        if back.is_none() {
            *back = wait(Duration::ZERO);
        }
        back.is_some()
    };
    loop {
        let line = terminal.line_until(&prompt, &done);
        let came = lock(&back).take();
        if let Some(outcome) = came {
            return outcome.map(Some);
        }
        let Some(line) = line else {
            return Ok(None);
        };
        match line.as_str() {
            "q" | "Q" => return Ok(None),
            "" => match wait(Duration::from_secs(3)) {
                Some(outcome) => return outcome.map(Some),
                None => say("The browser has not come back yet.\n"),
            },
            text => return pasted(text).map(Some),
        }
    }
}

/// `llx logout`: forget every saved sign-in.
pub(super) fn logout() -> Result<i32, Failure> {
    let mut any = false;
    let mut in_env = vec![];
    for provider in Provider::all() {
        let removed = provider
            .forget()
            .map_err(|error| Failure::Environment(format!("could not sign out: {error}")))?;
        if removed {
            any = true;
            error::print(&format!("{}: signed out\n", provider.name()))?;
        }
        if provider.key_in_env(None) {
            in_env.push(provider.name());
        }
    }
    if !any {
        error::print("No saved sign-in.\n")?;
    }
    // After what stdout says: a closed one has ended the command quietly by now.
    for name in in_env {
        error::note(&format!(
            "Note: {name}'s key is still in the environment; unset it to sign out fully.\n"
        ));
    }
    Ok(exit::OK)
}

#[cfg(test)]
#[path = "signin_tests.rs"]
mod tests;
