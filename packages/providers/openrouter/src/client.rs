//! The HTTP client to OpenRouter, and to any endpoint that speaks OpenAI's chat completions at
//! another address (DeepSeek, xAI, Groq, Mistral…: the catalog gives the address), over the
//! kit's [`Http`]. Its one agent keeps the connection alive across steps, so a decision costs a
//! request, not a handshake.

use std::time::{Duration, Instant};

use provider_kit::App;
use provider_kit::failure::{ApiError, message_of, reported, said, transient};
use provider_kit::http::{Http, HttpError, Web};
use provider_kit::retry::{Attempt, PAUSE, Resend, after, retried};
use serde_json::Value;

const BASE_URL: &str = "https://openrouter.ai/api/v1";
/// OpenAI's own API: it takes a prompt cache key, and its own name for the output limit.
const OPENAI_HOST: &str = "api.openai.com";
/// A long reply with high reasoning is slow.
const TIMEOUT: Duration = Duration::from_secs(600);
/// How long OpenRouter's stream may stay silent before it is given up: a connection a sleep or a
/// network change left dead. It sends a comment while a model thinks, so a live one is not quiet
/// this long; another host's answer is sent whole, silent until it is ready, and has no such limit.
const IDLE: Duration = Duration::from_secs(300);
/// A decision is short and cheap: one that hangs is given up on sooner, and asked once more.
const DECIDING: Duration = Duration::from_secs(90);

/// Which fields a host's chat completions take beside the common ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wire {
    /// OpenRouter: its `reasoning` object, its usage report, its sessions, cache marks on the
    /// messages, and a streamed reply.
    OpenRouter,
    /// OpenAI itself: `reasoning_effort`, `max_completion_tokens`, and the prompt cache's key.
    OpenAi,
    /// Any other host: `reasoning_effort` only; a field it does not know may be refused.
    Plain,
}

/// A client of OpenRouter's API, or of another host's chat completions, with its key.
pub struct OpenRouter {
    http: Box<dyn Http>,
    key: String,
    base: String,
    app: App,
    /// Which fields its requests take: OpenRouter's wherever OpenRouter is reached, OpenAI's own,
    /// or the plain ones any host that speaks the wire takes.
    wire: Wire,
    /// Who it calls, for errors: `OpenRouter`, `DeepSeek`.
    pub(crate) name: String,
    /// A request sent and not answered is sent once more (decisions); otherwise never: a reply
    /// being written is billed, and asking again would pay for it twice.
    resend: Resend,
    /// The first pause before a request is asked again (C2: a test's is zero).
    pause: Duration,
}

impl OpenRouter {
    /// OpenRouter itself, called with `key`.
    pub fn new(key: String, app: App) -> Self {
        Self::standing_in(BASE_URL, key, app)
    }

    /// OpenRouter at `base` (one standing in for it): it takes OpenRouter's fields wherever it is.
    pub fn standing_in(base: &str, key: String, app: App) -> Self {
        Self {
            wire: Wire::OpenRouter,
            http: Box::new(Web::new(app, TIMEOUT).idle(IDLE)),
            ..Self::at(base, key, app, crate::NAME)
        }
    }

    /// This client for decisions: a decision is short and cheap, so one that hangs
    /// is given up on sooner and asked once more.
    #[must_use]
    pub fn deciding(self) -> Self {
        Self {
            http: Box::new(Web::new(self.app, DECIDING)),
            resend: Resend::Once,
            ..self
        }
    }

    /// A client of the chat completions at `base`, called `name`.
    pub fn at(base: &str, key: String, app: App, name: &str) -> Self {
        let base = base.trim_end_matches('/').to_string();
        let host = (base.split("://").nth(1)).and_then(|rest| rest.split('/').next());
        let wire = if host == Some(OPENAI_HOST) {
            Wire::OpenAi
        } else {
            Wire::Plain
        };
        Self {
            http: Box::new(Web::new(app, TIMEOUT)),
            key,
            base,
            app,
            wire,
            name: name.to_string(),
            resend: Resend::Never,
            pause: PAUSE,
        }
    }

    pub(crate) fn wire(&self) -> Wire {
        self.wire
    }

    /// POST `body` to `path`, asked again on an answer a moment's wait may mend.
    pub(crate) fn post(
        &self,
        path: &str,
        body: &Value,
        deadline: Option<Instant>,
    ) -> Result<(Value, u64), ApiError> {
        let started = Instant::now();
        let url = format!("{}{path}", self.base);
        let bearer = format!("Bearer {}", self.key);
        let mut headers = vec![
            ("Authorization", bearer.as_str()),
            ("Content-Type", "application/json"),
        ];
        // OpenRouter's attribution: its logs and rankings name the app; the title alone makes
        // no app page, only beside the referer.
        if self.wire == Wire::OpenRouter {
            headers.extend([
                ("HTTP-Referer", self.app.site),
                ("X-OpenRouter-Title", self.app.name),
            ]);
        }
        let body = body.to_string();
        retried(self.pause, deadline, self.resend, || {
            let (status, text, asked) = match self.http.post(&url, &headers, &body, deadline) {
                // With the wait a rate limit or an overload asks for.
                Ok(reply) => {
                    let asked = after(reply.header("retry-after"));
                    (reply.status, reply.body, asked)
                }
                Err(HttpError::Unanswered(why)) => {
                    return Attempt::Unanswered(ApiError::Network(why));
                }
                // One that never got through is asked again.
                Err(HttpError::Unsent(why)) => return Attempt::Again(ApiError::Network(why)),
            };
            // Sent whole, or streamed (OpenRouter's chat completions): the same completion.
            let parsed: Option<Value> =
                (serde_json::from_str(&text).ok()).or_else(|| crate::stream::assembled(&text));
            let failed = |code: u16, message: String| {
                let error = ApiError::Http(code, message);
                // A 402 with a wait is OpenRouter's in-flight budget, of a moment; one with none,
                // the account's credits.
                match (transient(code) || (code == 402 && asked.is_some()), asked) {
                    (true, Some(wait)) => Attempt::After(error, wait),
                    (true, None) => Attempt::Again(error),
                    (false, _) => Attempt::Done(Err(error)),
                }
            };
            if status == 200 {
                // OpenRouter reports some upstream failures inside a 200 body.
                if let Some(error) = parsed.as_ref().and_then(reported) {
                    let code = error
                        .get("code")
                        .and_then(Value::as_u64)
                        .and_then(|code| u16::try_from(code).ok())
                        .unwrap_or(502);
                    // A stream that failed after its first words was billed for them: sent again
                    // only as `Resend` says, as one that went out unanswered.
                    let written = parsed.as_ref().and_then(|value| value.get("written"));
                    if written == Some(&Value::Bool(true)) && transient(code) {
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
            let message = said(parsed.as_ref(), &text);
            match status {
                // 403 is a permission, a guardrail or a moderation flag, not a key refused.
                401 => Attempt::Done(Err(ApiError::Auth(message))),
                _ => failed(status, message),
            }
        })
    }
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
