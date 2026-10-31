//! What the model providers share, so a new provider is mostly its wire format: an HTTP client on
//! the OS's own TLS ([`http`]), the pieces of an OAuth sign-in ([`oauth`], [`loopback`] and its
//! pages), private credential files ([`store`]), server-sent events ([`sse`]), when a request is
//! asked again ([`retry`]) and the prompt cache's key ([`cache`]). It implements no port and knows
//! no provider.

pub mod cache;
pub mod http;
pub mod loopback;
pub mod oauth;
mod page;
pub mod retry;
pub mod sse;
pub mod store;

/// The app calling a provider: how it is named in requests and on the sign-in page, and the
/// folder its credentials are saved in.
#[derive(Debug, Clone, Copy)]
pub struct App {
    /// "llx": attribution and the sign-in page.
    pub name: &'static str,
    /// "llx": the user agent and the config folder.
    pub slug: &'static str,
    pub version: &'static str,
    /// The app's home page: OpenRouter's attribution names it beside `name`.
    pub site: &'static str,
}
