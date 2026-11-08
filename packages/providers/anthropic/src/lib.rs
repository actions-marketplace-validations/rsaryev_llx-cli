//! A model provider over the Messages API: Anthropic's models, and any host that speaks it at the
//! address the catalog gives. Its own wire, not chat completions: Anthropic's OpenAI-compatible
//! endpoint caches no prompt and ignores the level, so a long conversation there pays its whole
//! history again at every turn. Here the request marks where its prompt cache ends
//! ([`request`]), sends the level and the thinking the model takes, and carries the model's signed
//! thinking back with its turn. The reply is streamed and put back together ([`stream`]), so a
//! call the output limit cut off keeps what it wrote (B7). It knows nothing of the app that uses
//! it beyond the [`provider_kit::App`] it is given.

pub mod chat;
pub mod client;
mod request;
mod stream;

#[cfg(test)]
mod testing;
