//! The tokens a ChatGPT sign-in issues, the account they belong to, and the file they are kept
//! in (`<config>/<app>/openai.json`, 0600). They are never printed, logged or shown in `Debug`.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use provider_kit::{App, oauth, store};
use serde_json::{Value, json};

const FILE: &str = "openai.json";
/// Where the ChatGPT claims sit in a token.
const AUTH_CLAIM: &str = "https://api.openai.com/auth";
/// A token that states no lifetime is taken to last an hour.
const LIFETIME: u64 = 3_600;
/// Refreshed this long before it lapses, so no request starts with a token about to expire.
const MARGIN: u64 = 300;

#[derive(Clone, PartialEq, Eq)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    /// The ChatGPT account every request names.
    pub account: String,
    /// When the access token lapses, in seconds since the epoch.
    pub expires: u64,
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tokens {{ account: {:?}, expires: {} }}",
            self.account, self.expires
        )
    }
}

impl Tokens {
    /// The tokens in a token endpoint's answer. A refresh may leave out a new refresh token (the
    /// old one stays valid), so it is kept from `previous`; the ID token serves only to name the
    /// account, and is not kept.
    pub(crate) fn from_reply(
        body: &Value,
        previous: Option<&Tokens>,
        now: u64,
    ) -> Result<Self, String> {
        let text = |key: &str| {
            body.get(key)
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        };
        let access = text("access_token").ok_or("OpenAI sent no access token")?;
        let refresh = text("refresh_token")
            .or_else(|| previous.map(|tokens| tokens.refresh.clone()))
            .ok_or("OpenAI sent no refresh token")?;
        let account = (text("id_token").as_deref())
            .and_then(account_of)
            .or_else(|| account_of(&access))
            .or_else(|| previous.map(|tokens| tokens.account.clone()))
            .ok_or("the ChatGPT sign-in names no account")?;
        let expires = oauth::claims(&access)
            .and_then(|claims| claims.get("exp").and_then(Value::as_u64))
            .unwrap_or_else(|| now + body["expires_in"].as_u64().unwrap_or(LIFETIME));
        Ok(Self {
            access,
            refresh,
            account,
            expires,
        })
    }

    /// Whether the access token lapses within the margin.
    pub(crate) fn stale(&self, now: u64) -> bool {
        now + MARGIN >= self.expires
    }

    fn to_json(&self) -> Value {
        json!({
            "access_token": self.access,
            "refresh_token": self.refresh,
            "account_id": self.account,
            "expires_at": self.expires,
        })
    }

    fn from_json(value: &Value) -> Option<Self> {
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        Some(Self {
            access: text("access_token")?,
            refresh: text("refresh_token")?,
            account: text("account_id")?,
            expires: value.get("expires_at")?.as_u64()?,
        })
    }
}

/// The ChatGPT account id a token's claims name.
fn account_of(token: &str) -> Option<String> {
    let claims = oauth::claims(token)?;
    let account = claims
        .get(AUTH_CLAIM)?
        .get("chatgpt_account_id")?
        .as_str()?;
    (!account.is_empty()).then(|| account.to_string())
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn path(app: App) -> Option<PathBuf> {
    store::path(app, FILE)
}

/// Where refreshed tokens are kept, and read back from when another process refreshed first.
pub(crate) trait Keep: Send + Sync {
    fn load(&self) -> Option<Tokens>;
    fn save(&self, tokens: &Tokens);
}

/// The tokens file of `App`.
pub(crate) struct Saved(pub(crate) App);

impl Keep for Saved {
    fn load(&self) -> Option<Tokens> {
        load(self.0)
    }

    fn save(&self, tokens: &Tokens) {
        // The new tokens are used either way; unsaved, the next process refreshes with a refresh
        // token that is spent, which the issuer refuses: the log says why that sign-in lapses.
        if let Err(error) = save(self.0, tokens) {
            log::warn!("the refreshed ChatGPT sign-in could not be saved: {error}");
        }
    }
}

/// The tokens a sign-in saved for `app`.
pub fn load(app: App) -> Option<Tokens> {
    let text = store::read(&path(app)?)?;
    Tokens::from_json(&serde_json::from_str(&text).ok()?)
}

pub fn save(app: App, tokens: &Tokens) -> std::io::Result<PathBuf> {
    let path = path(app).ok_or_else(|| std::io::Error::other("no home directory"))?;
    store::save(&path, &tokens.to_json().to_string())?;
    Ok(path)
}

/// Forget the saved tokens; whether there were any.
pub fn remove(app: App) -> std::io::Result<bool> {
    path(app).map_or(Ok(false), |path| store::remove(&path))
}

#[cfg(test)]
#[path = "tokens_tests.rs"]
mod tests;
