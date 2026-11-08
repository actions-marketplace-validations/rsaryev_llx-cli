//! A model provider: OpenRouter's chat models behind the chat port ([`chat::ChatModel`]), and any
//! OpenAI-compatible endpoint's through the same client, its decision models behind the decision
//! port ([`system_one::SystemOne`]), and its key: from the environment, pasted, or issued by a
//! sign-in. The models themselves are the catalog's (`models`). It knows nothing of the app that
//! uses it beyond the [`provider_kit::App`] it is given.

pub mod auth;
mod carry;
pub mod chat;
pub mod client;
pub mod level;
mod stream;
pub mod system_one;

/// How the provider is named to the person.
pub const NAME: &str = "OpenRouter";
