//! The OpenRouter key: `OPENROUTER_API_KEY`, else the one saved for the app (pasted, or issued
//! by a sign-in). Signing in is OpenRouter's PKCE flow in the browser: it ends with a permanent
//! API key, so nothing ever refreshes.

use std::path::PathBuf;
use std::time::Duration;

use provider_kit::http::encode;
use provider_kit::loopback::{self, Callback, Listen};
use provider_kit::{App, oauth, store};
use serde_json::{Value, json};

const AUTHORIZE: &str = "https://openrouter.ai/auth";
/// The environment variable a key comes from first.
pub const KEY_VARIABLE: &str = "OPENROUTER_API_KEY";

/// `<config>/<app slug>/credentials`.
pub fn credentials_path(app: App) -> Option<PathBuf> {
    store::path(app, "credentials")
}

/// A sign-in under way: the address to open, and the callback waiting for the browser.
pub struct Login {
    address: String,
    callback: Callback,
    verifier: String,
}

impl Login {
    /// Where the person signs in.
    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn start(app: App) -> Result<Self, String> {
        let (verifier, challenge) = oauth::pkce()?;
        // OpenRouter sends no `state` back; a random route keeps stray requests from completing
        // the sign-in.
        let path = format!("/oauth/callback/{}", oauth::random(12)?);
        let callback = loopback::listen(Listen {
            ports: &[0],
            path: &path,
            state: None,
            provider: crate::NAME,
            app: app.name,
        })?;
        let address = format!(
            "{AUTHORIZE}?callback_url={}&code_challenge={challenge}&code_challenge_method=S256",
            encode(callback.redirect())
        );
        Ok(Self {
            address,
            callback,
            verifier,
        })
    }

    /// The code the browser brought back, once it has (within `timeout`).
    pub fn wait(&self, timeout: Duration) -> Option<Result<String, String>> {
        self.callback.wait(timeout)
    }

    /// The code in the address the browser ended on, or the code alone.
    pub fn pasted(&self, input: &str) -> Result<String, String> {
        let pasted = oauth::pasted(input);
        match (pasted.code, pasted.error) {
            (_, Some(error)) => Err(format!("OpenRouter refused the sign-in: {error}")),
            (Some(code), None) => Ok(code),
            (None, None) => Err("no authorization code in what was pasted".into()),
        }
    }

    /// Exchange `code` for the key it issues.
    pub fn finish(&self, client: &crate::client::OpenRouter, code: &str) -> Result<String, String> {
        let body = json!({"code": code, "code_verifier": self.verifier,
            "code_challenge_method": "S256"});
        let (value, _) = client
            .post("/auth/keys", &body, None)
            .map_err(|e| e.to_string())?;
        value
            .get("key")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "OpenRouter sent no API key".into())
    }
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
