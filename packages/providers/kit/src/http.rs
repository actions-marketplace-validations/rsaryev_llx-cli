//! HTTP as the providers use it: a POST of a body with headers, answered with a status and the
//! whole body. Behind [`Http`], so a provider's requests are tested with a fake and no network;
//! a GET ([`Web::get`]) for the catalog, read the same way.

use std::time::{Duration, Instant};

use crate::App;

/// What a server answered: its status, its headers (names in lower case) and its body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Reply {
    /// The value of the header `name`, in any case.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        (self.headers.iter())
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

pub trait Http: Send + Sync {
    /// POST `body` to `address` with `headers`; `Err` when no whole answer came. Not waited for
    /// past `deadline`, when there is one.
    fn post(
        &self,
        address: &str,
        headers: &[(&str, &str)],
        body: &str,
        deadline: Option<Instant>,
    ) -> Result<Reply, HttpError>;
}

/// Why no answer came, as a caller acts on it: whether the request may be sent again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    /// It did not get through (offline, no such host, refused): sending it again is safe.
    Unsent(String),
    /// It went out and no whole answer came back (the time ran out, the connection broke, the
    /// answer was cut short or too long): the server may have acted on it, and billed it.
    Unanswered(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsent(why) | Self::Unanswered(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for HttpError {}

/// Why a request is not sent once its deadline has passed.
const OUT_OF_TIME: &str = "the time limit ran out before the request";

/// The longest body read: a long streamed reply is a few megabytes at most.
const BODY_LIMIT: u64 = 64 << 20;

/// An agent on the OS's own TLS and trust store (Security.framework on macOS, OpenSSL on Linux): no
/// bundled TLS stack or root certificates in the binary, and corporate roots work. Statuses are
/// answers, not errors. One agent keeps its connections alive across requests.
fn agent(app: App, timeout: Duration) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(timeout))
        .user_agent(format!("{}/{}", app.slug, app.version))
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::NativeTls)
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build();
    ureq::Agent::new_with_config(config)
}

/// [`Http`] over the network: one agent, its connections kept alive across requests.
#[derive(Clone)]
pub struct Web {
    agent: ureq::Agent,
    timeout: Duration,
    /// The longest silence a streamed answer may keep before it is given up; `None`: no limit.
    idle: Option<Duration>,
}

impl Web {
    /// Each request given `timeout` whole, from its connection to the end of its answer.
    pub fn new(app: App, timeout: Duration) -> Self {
        Self {
            agent: agent(app, timeout),
            timeout,
            idle: None,
        }
    }

    /// The same client, giving up an answer that sends nothing for `idle`: a stream whose
    /// connection died (a laptop slept through it) ends there, not at the whole timeout. Only for
    /// a streamed answer: one sent whole is silent until it is ready.
    #[must_use]
    pub fn idle(mut self, idle: Duration) -> Self {
        self.idle = Some(idle);
        self
    }

    /// GET `address` with `headers`, given the client's whole timeout; `Err` as for a POST. Its
    /// answer is sent whole (the catalog): no silence limit.
    pub fn get(&self, address: &str, headers: &[(&str, &str)]) -> Result<Reply, HttpError> {
        let mut request = self.agent.get(address);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        reply(request.call().map_err(|error| sent_or_not(&error))?)
    }
}

impl Http for Web {
    fn post(
        &self,
        address: &str,
        headers: &[(&str, &str)],
        body: &str,
        deadline: Option<Instant>,
    ) -> Result<Reply, HttpError> {
        // The time left, capped by the client's own limit, is this request's: ureq takes a
        // per-request `config().timeout_global(..)`, so the agent and its connections are kept.
        let timeout = match deadline {
            None => self.timeout,
            Some(at) => {
                let left = at.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Err(HttpError::Unsent(OUT_OF_TIME.into()));
                }
                left.min(self.timeout)
            }
        };
        let mut request = self.agent.post(address);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let request = (request.config())
            .timeout_global(Some(timeout))
            .timeout_per_read(self.idle)
            .build();
        reply(request.send(body).map_err(|error| sent_or_not(&error))?)
    }
}

/// A response read whole: its status, its headers (names in lower case), its body up to
/// [`BODY_LIMIT`].
fn reply(mut response: ureq::http::Response<ureq::Body>) -> Result<Reply, HttpError> {
    let status = response.status().as_u16();
    let headers = (response.headers().iter())
        .filter_map(|(name, value)| {
            let value = value.to_str().ok()?;
            Some((name.as_str().to_ascii_lowercase(), value.to_string()))
        })
        .collect();
    let body = (response.body_mut().with_config().limit(BODY_LIMIT))
        .read_to_string()
        .map_err(|error| HttpError::Unanswered(format!("the answer was not read: {error}")))?;
    Ok(Reply {
        status,
        headers,
        body,
    })
}

/// Whether a failed send left the request unsent: it failed before a byte went out (no such host,
/// refused, a failed handshake, a connect that timed out), else it may have been acted on.
fn sent_or_not(error: &ureq::Error) -> HttpError {
    use std::io::ErrorKind::{
        AddrNotAvailable, ConnectionRefused, HostUnreachable, NetworkUnreachable,
    };
    use ureq::{Error, Timeout};
    // Any other failure came after the request was written.
    let before = matches!(
        error,
        Error::HostNotFound
            | Error::ConnectionFailed
            | Error::BadUri(_)
            | Error::Http(_)
            | Error::Tls(_)
            | Error::NativeTls(_)
            | Error::Timeout(Timeout::Connect | Timeout::Resolve)
    ) || matches!(error, Error::Io(io) if matches!(
        io.kind(),
        ConnectionRefused | AddrNotAvailable | HostUnreachable | NetworkUnreachable
    ));
    if before {
        HttpError::Unsent(error.to_string())
    } else {
        HttpError::Unanswered(error.to_string())
    }
}

/// `pairs` as an `application/x-www-form-urlencoded` body.
pub fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{}={}", encode(key), encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// `text` percent-encoded for a query or a form: only unreserved characters stay as they are.
pub fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// `text` with its percent escapes (and `+` as a space) decoded.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        // Two hex digits, checked as such: `from_str_radix` would also take a sign (`%+1`).
        let escaped = (bytes.get(at + 1..at + 3))
            .filter(|hex| hex.iter().all(u8::is_ascii_hexdigit))
            .and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok());
        match (bytes[at], escaped) {
            (b'%', Some(byte)) => {
                out.push(byte);
                at += 3;
                continue;
            }
            (b'+', _) => out.push(b' '),
            (byte, _) => out.push(byte),
        }
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The value of `key` in the query of `target` (a path or a whole address), decoded.
pub fn query_value(target: &str, key: &str) -> Option<String> {
    let (_, query) = target.split_once('?')?;
    let query = query.split('#').next().unwrap_or(query);
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        (name == key).then(|| decode(value))
    })
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
