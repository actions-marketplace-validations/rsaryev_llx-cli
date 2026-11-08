//! The chat port over the Responses API: the ChatGPT backend's, signed in with ChatGPT, or
//! OpenAI's own with a key. Every reply is streamed and nothing is stored (`stream`,
//! `store: false`), so each request carries the whole conversation, its sealed reasoning with it;
//! the standing instructions go first as a developer message. A sign-in's access token is
//! refreshed before it lapses, and once more when a request is refused, within the request's
//! deadline (`signin.rs`).

use std::time::{Duration, Instant};

use contracts::catalog::Selection;
use contracts::chat::{Chat, ChatError, Completion, Request};
use provider_kit::App;
use provider_kit::http::{Http, HttpError, Web};
use provider_kit::retry::{Attempt, PAUSE, Resend, TRANSIENT, retried};
use serde_json::Value;

use crate::auth;
use crate::tokens::{Keep, Saved, Tokens};
use signin::Renew;

mod offer;
mod reply;
mod request;
mod route;
mod signin;
pub use offer::Texts;
use reply::{failed_for_load, read};
use request::body;

const BASE: &str = "https://chatgpt.com/backend-api/codex";
/// How long the backend may take over one streamed reply (high reasoning is slow).
const TIMEOUT: Duration = Duration::from_secs(600);
/// How long its stream may stay silent before it is given up and asked once more: a connection a
/// sleep or a network change left dead. Codex's `stream_idle_timeout_ms`; a model may think
/// for minutes without a byte.
const IDLE: Duration = Duration::from_secs(300);

/// How the requests are signed.
enum Access {
    /// A ChatGPT sign-in: the backend, billed to the plan.
    SignIn(signin::SignIn),
    /// An API key: OpenAI's own API, billed per token.
    Key(String),
}

/// OpenAI's models over the Responses API: the ChatGPT backend's with a sign-in, the API's with a
/// key (where chat completions refuse function tools with a level for the newest models, and keep
/// no reasoning between turns).
pub struct ChatModel {
    http: Box<dyn Http>,
    access: Access,
    /// Where the Responses API is: the backend's, or the API's the catalog gives.
    base: String,
    app: App,
    selection: Selection,
    /// How a tool of free text (a patch) is offered.
    texts: Texts,
    /// Each task's machine, as the backend named it.
    routes: route::Routes,
    /// The first pause before a request is asked again (C2: a test's is zero).
    pause: Duration,
}

impl ChatModel {
    /// The backend for `app`, signed in with `tokens`, refreshed ones saved in its tokens file;
    /// tools of free text offered as `texts` says (`custom` ones for a model that takes those).
    pub fn new(app: App, tokens: Tokens, selection: Selection, texts: Texts) -> Self {
        let http = Box::new(Web::new(app, TIMEOUT).idle(IDLE));
        let issuer = Box::new(auth::issuer(app));
        let model = Self::on(http, issuer, Box::new(Saved(app)), app, tokens, selection);
        Self { texts, ..model }
    }

    /// OpenAI's API at `base` (the catalog's address), called with `key`.
    pub fn with_key(app: App, key: String, base: &str, selection: Selection, texts: Texts) -> Self {
        let http = Box::new(Web::new(app, TIMEOUT).idle(IDLE));
        Self::keyed(http, app, key, base, selection, texts)
    }

    pub(crate) fn keyed(
        http: Box<dyn Http>,
        app: App,
        key: String,
        base: &str,
        selection: Selection,
        texts: Texts,
    ) -> Self {
        Self {
            http,
            access: Access::Key(key),
            base: base.trim_end_matches('/').to_string(),
            app,
            selection,
            texts,
            routes: route::Routes::default(),
            pause: PAUSE,
        }
    }

    /// The backend over `http`, its tokens refreshed at `issuer` and kept in `keep`.
    pub(crate) fn on(
        http: Box<dyn Http>,
        issuer: Box<dyn Http>,
        keep: Box<dyn Keep>,
        app: App,
        tokens: Tokens,
        selection: Selection,
    ) -> Self {
        Self {
            http,
            access: Access::SignIn(signin::SignIn::new(issuer, keep, tokens)),
            base: BASE.to_string(),
            app,
            selection,
            texts: Texts::Functions,
            routes: route::Routes::default(),
            pause: PAUSE,
        }
    }

    /// Who answers, for what is said of a failure.
    fn host(&self) -> &'static str {
        match self.access {
            Access::SignIn(_) => "ChatGPT",
            Access::Key(_) => "OpenAI",
        }
    }
}

impl Chat for ChatModel {
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        let session = route::session(request.conversation);
        let task = (session.clone(), request.task);
        let offer = offer::Offer::new(request.tools, self.texts, request.conversation);
        let selection = self.selection.or_at(request.effort);
        let body = body(&selection, request, &offer, &session).to_string();
        let address = format!("{}/responses", self.base);
        let host = self.host();
        let started = Instant::now();
        let mut renewed = false;
        // A stream cut or timed out may have been billed, so it is asked once more at most:
        // nothing of it was acted on, and the backend stores nothing of a request.
        retried(self.pause, request.deadline, Resend::Once, || {
            let (bearer, account) = match &self.access {
                Access::SignIn(signin) => match signin.tokens(Renew::IfStale, request.deadline) {
                    Ok(tokens) => (format!("Bearer {}", tokens.access), Some(tokens.account)),
                    Err(error) => return Attempt::Done(Err(error)),
                },
                Access::Key(key) => (format!("Bearer {key}"), None),
            };
            let mut headers = vec![
                ("Authorization", bearer.as_str()),
                ("Content-Type", "application/json"),
                ("Accept", "text/event-stream"),
            ];
            // The backend's own: the account, who calls, the session, and the machine a task's
            // first reply named (sticky routing, as Codex sends them).
            let routed = account.as_ref().and_then(|_| self.routes.token(&task));
            if let Some(account) = &account {
                headers.extend([
                    ("chatgpt-account-id", account.as_str()),
                    ("originator", self.app.slug),
                    ("session-id", session.as_str()),
                ]);
            }
            if let Some(token) = &routed {
                headers.push((route::HEADER, token));
            }
            let reply = match self.http.post(&address, &headers, &body, request.deadline) {
                Ok(reply) => reply,
                Err(HttpError::Unsent(why)) => {
                    return Attempt::Again(ChatError::Unavailable(format!(
                        "{host} is unreachable: {why}"
                    )));
                }
                Err(HttpError::Unanswered(why)) => {
                    let error = ChatError::Unavailable(format!("{host} did not answer: {why}"));
                    return Attempt::Unanswered(error);
                }
            };
            if reply.status == 200
                && let Some(error) = failed_for_load(&reply.body)
            {
                return Attempt::Again(error);
            }
            if let (401, false, Access::SignIn(signin)) = (reply.status, renewed, &self.access) {
                renewed = true;
                return match signin.tokens(Renew::Now, request.deadline) {
                    Ok(_) => Attempt::Again(ChatError::Unavailable(refusal(401, &reply.body))),
                    Err(error) => Attempt::Done(Err(error)),
                };
            }
            match reply.status {
                200 => Attempt::Done({
                    route::trace(&session, routed.as_deref(), &reply);
                    self.routes.keep(&task, &reply);
                    read(&reply.body, self.selection.model().id.as_str()).map(|mut completion| {
                        completion.calls = (completion.calls.into_iter())
                            .map(|call| offer.incoming(call))
                            .collect();
                        completion.usage.millis =
                            u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                        completion
                    })
                }),
                // A 429 is a key's rate limit of a moment; with a sign-in it is the plan's usage
                // limit, hours away: not worth a second try.
                429 if matches!(self.access, Access::Key(_)) => {
                    Attempt::Again(ChatError::Unavailable(rejected(&reply.body)))
                }
                status if TRANSIENT.contains(&status) => {
                    Attempt::Again(ChatError::Unavailable(refusal(status, &reply.body)))
                }
                status if matches!(self.access, Access::Key(_)) => {
                    let message = rejected(&reply.body);
                    Attempt::Done(Err(ChatError::Unavailable(format!(
                        "OpenAI answered {status}: {message}"
                    ))))
                }
                status => Attempt::Done(Err(ChatError::Unavailable(refusal(status, &reply.body)))),
            }
        })
    }
}

/// The host's own words for a refusal, where it sent some.
fn rejected(body: &str) -> String {
    let value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    value
        .pointer("/error/message")
        .or_else(|| value.get("detail"))
        .and_then(Value::as_str)
        .map_or_else(|| body.chars().take(300).collect(), str::to_string)
}

/// Why the backend refused, in words: its own message where it sent one.
fn refusal(status: u16, body: &str) -> String {
    let message = rejected(body);
    match status {
        429 => format!("the ChatGPT plan's usage limit is reached: {message}"),
        401 | 403 => format!("ChatGPT refused the sign-in ({status}): {message}; /login again"),
        _ => format!("ChatGPT answered {status}: {message}"),
    }
}

#[cfg(test)]
#[path = "responses_tests.rs"]
mod tests;
