//! Signing in with ChatGPT: the OAuth authorization-code flow with PKCE, in the system browser,
//! back to a local callback on a port the issuer allows (1455, else 1457). The code is exchanged
//! for an access, a refresh and an ID token; the access token is refreshed before it lapses.
//! Where the browser cannot reach the callback (another machine), the person pastes the address
//! it ended on, and its `state` is checked the same way.

use std::time::{Duration, Instant};

use provider_kit::http::{Http, Web, form};
use provider_kit::loopback::{self, Callback, Listen};
use provider_kit::{App, oauth};
use serde_json::{Value, json};

use crate::tokens::Tokens;

pub(crate) const ISSUER: &str = "https://auth.openai.com";
/// The public client the issuer knows terminal sign-ins by.
pub(crate) const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
/// The callback ports the issuer accepts redirects to.
const PORTS: [u16; 2] = [1455, 1457];
const CALLBACK: &str = "/auth/callback";
const SCOPE: &str = "openid profile email offline_access";
/// How long the token endpoint may take: its answer is short.
const TOKEN_TIMEOUT: Duration = Duration::from_secs(60);

/// The client the token endpoint is called with.
pub(crate) fn issuer(app: App) -> Web {
    Web::new(app, TOKEN_TIMEOUT)
}

/// A sign-in under way: the address to open, and the callback waiting for the browser.
pub struct Login {
    app: App,
    address: String,
    callback: Callback,
    verifier: String,
    state: String,
}

impl Login {
    /// Where the person signs in.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Listen for the callback and make the sign-in address.
    pub fn start(app: App) -> Result<Self, String> {
        Self::listen(app, &PORTS)
    }

    fn listen(app: App, ports: &[u16]) -> Result<Self, String> {
        let (verifier, challenge) = oauth::pkce()?;
        let state = oauth::random(32)?;
        let callback = loopback::listen(Listen {
            ports,
            path: CALLBACK,
            state: Some(&state),
            provider: crate::NAME,
            app: app.name,
        })?;
        let address = authorize(app, callback.redirect(), &challenge, &state);
        Ok(Self {
            app,
            address,
            callback,
            verifier,
            state,
        })
    }

    /// The code the browser brought back, once it has (within `timeout`).
    pub fn wait(&self, timeout: Duration) -> Option<Result<String, String>> {
        self.callback.wait(timeout)
    }

    /// The code in what the person pasted: the address the browser ended on, or the code alone.
    /// An address from another sign-in (its `state` differs) is refused.
    pub fn pasted(&self, input: &str) -> Result<String, String> {
        let pasted = oauth::pasted(input);
        if let Some(error) = pasted.error {
            return Err(format!("ChatGPT refused the sign-in: {error}"));
        }
        if pasted.state.is_some_and(|state| state != self.state) {
            return Err("that address belongs to another sign-in (its state differs)".into());
        }
        pasted
            .code
            .ok_or_else(|| "no authorization code in what was pasted".into())
    }

    /// Exchange `code` for the tokens.
    pub fn finish(&self, code: &str) -> Result<Tokens, String> {
        self.exchanged(&issuer(self.app), code)
    }

    /// [`Login::finish`] at the token endpoint `http` reaches.
    pub(crate) fn exchanged(&self, http: &dyn Http, code: &str) -> Result<Tokens, String> {
        exchange(
            http,
            code,
            &self.verifier,
            self.callback.redirect(),
            crate::tokens::now(),
        )
    }
}

/// The address the person signs in at.
pub(crate) fn authorize(app: App, redirect: &str, challenge: &str, state: &str) -> String {
    let query = form(&[
        ("response_type", "code"),
        ("client_id", CLIENT_ID),
        ("redirect_uri", redirect),
        ("scope", SCOPE),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
        ("state", state),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("originator", app.slug),
    ]);
    format!("{ISSUER}/oauth/authorize?{query}")
}

/// The authorization code for tokens (a form, as the token endpoint takes it).
pub(crate) fn exchange(
    http: &dyn Http,
    code: &str,
    verifier: &str,
    redirect: &str,
    now: u64,
) -> Result<Tokens, String> {
    let body = form(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect),
        ("client_id", CLIENT_ID),
        ("code_verifier", verifier),
    ]);
    let content = ("Content-Type", "application/x-www-form-urlencoded");
    let reply = token(http, content, &body, "exchange", None)?;
    Tokens::from_reply(&reply, None, now)
}

/// Fresh tokens for `tokens` (JSON, as the token endpoint takes a refresh), by `deadline`.
pub(crate) fn refresh(
    http: &dyn Http,
    tokens: &Tokens,
    now: u64,
    deadline: Option<Instant>,
) -> Result<Tokens, String> {
    let body = json!({
        "client_id": CLIENT_ID,
        "grant_type": "refresh_token",
        "refresh_token": tokens.refresh,
    });
    let content = ("Content-Type", "application/json");
    let reply = token(http, content, &body.to_string(), "refresh", deadline)?;
    Tokens::from_reply(&reply, Some(tokens), now)
}

/// One token request, the `operation` it is and the deadline it is held to; its error said in
/// words the person can act on, with no secret in them.
fn token(
    http: &dyn Http,
    content: (&str, &str),
    body: &str,
    operation: &str,
    deadline: Option<Instant>,
) -> Result<Value, String> {
    let address = format!("{ISSUER}/oauth/token");
    let headers = [content, ("Accept", "application/json")];
    let reply = http
        .post(&address, &headers, body, deadline)
        .map_err(|error| format!("the ChatGPT sign-in could not be reached: {error}"))?;
    let value: Value = serde_json::from_str(&reply.body).unwrap_or(Value::Null);
    if reply.status == 200 {
        return Ok(value);
    }
    let code = value
        .pointer("/error/code")
        .or_else(|| value.get("error"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let again = "sign in again with /login";
    Err(match code {
        "refresh_token_expired" => format!("the ChatGPT sign-in has expired: {again}"),
        "refresh_token_reused" => format!("the ChatGPT sign-in was used elsewhere: {again}"),
        "invalid_grant" => format!("the ChatGPT sign-in is no longer valid: {again}"),
        "refresh_token_invalidated" => format!("the ChatGPT sign-in was revoked: {again}"),
        _ => format!(
            "the ChatGPT token {operation} failed ({}): {again}",
            reply.status
        ),
    })
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
