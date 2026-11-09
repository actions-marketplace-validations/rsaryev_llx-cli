//! A host's failure as the providers that speak JSON over HTTP report it: why there is no
//! answer ([`ApiError`]), whether a wait may mend a status, and what an error in a reply says.

use std::fmt;

use serde_json::Value;

use crate::retry::TRANSIENT;

/// Why a request to a host came to no answer.
#[derive(Debug, PartialEq, Eq)]
pub enum ApiError {
    /// The key was refused: `/login` again, or fix the key's variable.
    Auth(String),
    /// The host or the model answered with an error.
    Http(u16, String),
    /// No answer: offline, DNS, timeout.
    Network(String),
    /// An answer that does not have the expected shape.
    Invalid(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auth(message) => write!(
                f,
                "refused the key: {message}; type /login, or check the key's variable"
            ),
            Self::Http(status, message) => write!(f, "answered {status}: {message}"),
            Self::Network(message) => write!(f, "unreachable: {message}"),
            Self::Invalid(message) => write!(f, "unexpected answer: {message}"),
        }
    }
}

/// Whether a wait may mend `status` from a host called with a key: a 429 is a rate limit of a
/// moment, as are the statuses every host means so.
#[must_use]
pub fn transient(status: u16) -> bool {
    status == 429 || TRANSIENT.contains(&status)
}

/// The error a reply reports, where it reports one: an `error` that is `null` is none, as a
/// compatible host may send it beside a completion that went well.
#[must_use]
pub fn reported(reply: &Value) -> Option<&Value> {
    reply.get("error").filter(|error| !error.is_null())
}

/// What `error` says, its first 400 characters.
#[must_use]
pub fn message_of(error: &Value) -> String {
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("no message");
    message.chars().take(400).collect()
}

/// What a failed answer says: the message of the error it reports, else the start of its body.
#[must_use]
pub fn said(parsed: Option<&Value>, body: &str) -> String {
    parsed
        .and_then(reported)
        .map_or_else(|| body.chars().take(300).collect(), message_of)
}

#[cfg(test)]
#[path = "failure_tests.rs"]
mod tests;
