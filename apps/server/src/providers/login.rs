//! A provider's sign-in in the browser, for `/login`: with `providers.rs`, the one place that
//! knows each provider package's own (K1, A5). Where the person is sent, how the code comes
//! back, and what keeping it says, with no secret in the words.

use std::time::Duration;

use provider_kit::store::masked;

use super::{Provider, Vars};
use crate::compose::APP;

/// A browser sign-in begun.
pub(crate) enum Login {
    /// ChatGPT's sign-in: its tokens, kept.
    ChatGpt(openai::auth::Login),
    /// OpenRouter's sign-in: a key made for llx, kept.
    OpenRouter(openrouter::auth::Login, openrouter::client::OpenRouter),
}

impl Provider {
    /// A sign-in in the browser begun, where the provider has one; `vars` is the session's
    /// environment, where a server standing in for OpenRouter is named.
    pub(crate) fn browser_login(&self, vars: Vars) -> Option<Result<Login, String>> {
        Some(match self.id() {
            "openai" => openai::auth::Login::start(APP).map(Login::ChatGpt),
            "openrouter" => openrouter::auth::Login::start(APP).map(|login| {
                Login::OpenRouter(login, super::openrouter_client(String::new(), vars))
            }),
            _ => return None,
        })
    }
}

impl Login {
    /// Where the person is sent.
    pub(crate) fn address(&self) -> &str {
        match self {
            Self::ChatGpt(login) => login.address(),
            Self::OpenRouter(login, _) => login.address(),
        }
    }

    /// The code, once the browser came back within `timeout`.
    pub(crate) fn wait(&self, timeout: Duration) -> Option<Result<String, String>> {
        match self {
            Self::ChatGpt(login) => login.wait(timeout),
            Self::OpenRouter(login, _) => login.wait(timeout),
        }
    }

    /// The code in the address the browser ended on, pasted.
    pub(crate) fn pasted(&self, input: &str) -> Result<String, String> {
        match self {
            Self::ChatGpt(login) => login.pasted(input),
            Self::OpenRouter(login, _) => login.pasted(input),
        }
    }

    /// The code exchanged and kept: what was saved, in words with no secret in them.
    pub(crate) fn finish(&self, code: &str) -> Result<String, String> {
        match self {
            Self::ChatGpt(login) => {
                let tokens = login.finish(code)?;
                let file = openai::tokens::save(APP, &tokens).map_err(|e| e.to_string())?;
                Ok(format!(
                    "Signed in to ChatGPT account {}; saved in {}.",
                    tokens.account,
                    file.display()
                ))
            }
            Self::OpenRouter(login, client) => {
                let key = login.finish(client, code)?;
                let file = Provider::openrouter()
                    .save_key(&key)
                    .map_err(|e| e.to_string())?;
                Ok(format!(
                    "Signed in: the key {} is saved in {}.",
                    masked(&key),
                    file.display()
                ))
            }
        }
    }
}
