//! The HTTP client to the Messages API at an address, with its key, over the kit's [`Http`]. One
//! agent keeps the connection alive across a run's requests.

use std::time::{Duration, Instant};

use provider_kit::App;
use provider_kit::http::{Http, HttpError, Web};
use provider_kit::retry::{Attempt, PAUSE, Resend, TRANSIENT, retried};
use serde_json::Value;

/// The version of the API the requests are written for.
const VERSION: &str = "2023-06-01";
/// A long reply with high effort is slow.
const TIMEOUT: Duration = Duration::from_secs(600);
/// How long the stream may stay silent before it is given up: it sends a ping while the model
/// thinks, so a live one is not quiet this long; a connection a sleep or a network change left
/// dead is.
const IDLE: Duration = Duration::from_secs(300);

#[derive(Debug, PartialEq)]
pub(crate) enum ApiError {
    /// The key was refused: `/login` again, or fix the key's variable.
    Auth(String),
    /// The host or the model answered with an error.
    Http(u16, String),
    /// No answer: offline, DNS, timeout.
    Network(String),
    /// An answer that does not have the expected shape.
    Invalid(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

/// A client of the Messages API at one address.
pub struct Messages {
    http: Box<dyn Http>,
    key: String,
    base: String,
    /// Who it calls, for errors: `Anthropic`.
    pub(crate) name: String,
    /// The first pause before a request is asked again (C2: a test's is zero).
    pause: Duration,
}

impl Messages {
    /// The Messages API at `base` (the catalog's address, `…/v1`), called `name`, with `key`.
    pub fn new(base: &str, key: String, app: App, name: &str) -> Self {
        Self::over(Box::new(Web::new(app, TIMEOUT).idle(IDLE)), base, key, name)
    }

    pub(crate) fn over(http: Box<dyn Http>, base: &str, key: String, name: &str) -> Self {
        Self {
            http,
            key,
            base: base.trim_end_matches('/').to_string(),
            name: name.to_string(),
            pause: PAUSE,
        }
    }

    #[cfg(test)]
    pub(crate) fn unpaused(self) -> Self {
        Self {
            pause: Duration::ZERO,
            ..self
        }
    }

    /// POST `body` to `/messages`, asked again on an answer a moment's wait may mend; the message
    /// it answered and how long it took. One that went out unanswered is never sent again: a reply
    /// being written is billed, and asking again would pay for it twice.
    pub(crate) fn post(
        &self,
        body: &Value,
        deadline: Option<Instant>,
    ) -> Result<(Value, u64), ApiError> {
        let started = Instant::now();
        let url = format!("{}/messages", self.base);
        let headers = [
            ("x-api-key", self.key.as_str()),
            ("anthropic-version", VERSION),
            ("content-type", "application/json"),
        ];
        let body = body.to_string();
        retried(self.pause, deadline, Resend::Never, || {
            let (status, text) = match self.http.post(&url, &headers, &body, deadline) {
                Ok(reply) => (reply.status, reply.body),
                Err(HttpError::Unanswered(why)) => {
                    return Attempt::Unanswered(ApiError::Network(why));
                }
                Err(HttpError::Unsent(why)) => return Attempt::Again(ApiError::Network(why)),
            };
            let parsed: Option<Value> =
                (serde_json::from_str(&text).ok()).or_else(|| crate::stream::assembled(&text));
            let failed = |code: u16, message: String| {
                let error = ApiError::Http(code, message);
                match transient(code) {
                    true => Attempt::Again(error),
                    false => Attempt::Done(Err(error)),
                }
            };
            if status == 200 {
                // An error the stream reported midway: one after its first words was billed.
                if let Some(error) = parsed.as_ref().and_then(|value| value.get("error")) {
                    let code = overloaded(error);
                    let written = parsed.as_ref().and_then(|value| value.get("written"));
                    if written == Some(&Value::Bool(true)) {
                        return Attempt::Unanswered(ApiError::Http(code, message_of(error)));
                    }
                    return failed(code, message_of(error));
                }
                let millis = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                return Attempt::Done(
                    (parsed.map(|value| (value, millis)))
                        .ok_or_else(|| ApiError::Invalid("not JSON".into())),
                );
            }
            let message = parsed
                .as_ref()
                .and_then(|value| value.get("error"))
                .map(message_of)
                .unwrap_or_else(|| text.chars().take(300).collect());
            match status {
                401 | 403 => Attempt::Done(Err(ApiError::Auth(message))),
                _ => failed(status, message),
            }
        })
    }
}

/// The status an error inside a stream stands for: an overloaded host as its own 529, anything
/// else as a failure of the host.
fn overloaded(error: &Value) -> u16 {
    match error.get("type").and_then(Value::as_str) {
        Some("overloaded_error") => 529,
        Some("rate_limit_error") => 429,
        _ => 500,
    }
}

/// Whether a wait may mend `status`: a rate limit of a moment, or what every host means so.
fn transient(status: u16) -> bool {
    status == 429 || TRANSIENT.contains(&status)
}

fn message_of(error: &Value) -> String {
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("no message");
    message.chars().take(400).collect()
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
