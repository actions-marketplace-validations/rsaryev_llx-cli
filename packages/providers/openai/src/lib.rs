//! A model provider: OpenAI's models through a ChatGPT sign-in, billed to the person's plan, or
//! an API key, billed per token.
//! - [`auth`]: signing in (PKCE in the browser, a local callback) and refreshing the tokens.
//! - [`tokens`]: the tokens, the account they belong to, and the file they are kept in.
//! - [`responses`]: the chat port over the Responses API: the ChatGPT backend's, or OpenAI's own.
//!
//! It knows nothing of the app that uses it beyond the [`provider_kit::App`] it is given.

pub mod auth;
pub mod responses;
#[cfg(test)]
mod testing;
pub mod tokens;

/// How the provider is named to the person.
pub(crate) const NAME: &str = "OpenAI";
