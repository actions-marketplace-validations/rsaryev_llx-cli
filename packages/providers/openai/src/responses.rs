//! The chat port over the Responses API of the ChatGPT backend, signed in with ChatGPT. The
//! backend streams every reply and stores nothing (`stream`, `store: false`), so each request
//! carries the whole conversation; the standing instructions go first as a developer message.
//! The access token is refreshed before it lapses, and once more when a request is refused,
//! within the request's deadline.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use contracts::catalog::Selection;
use contracts::chat::{Chat, ChatError, Completion, Ending, Request, ToolCall, Usage};
use provider_kit::http::{Http, HttpError, Web};
use provider_kit::retry::{Attempt, PAUSE, Resend, TRANSIENT, retried};
use provider_kit::{App, sse};
use serde_json::{Value, json};

use crate::auth;
use crate::tokens::{self, Keep, Saved, Tokens};

mod offer;
mod request;
mod route;
pub use offer::Texts;
use offer::custom_call;
use request::body;

const BASE: &str = "https://chatgpt.com/backend-api/codex";
/// How long the backend may take over one streamed reply (high reasoning is slow).
const TIMEOUT: Duration = Duration::from_secs(600);
/// How long its stream may stay silent before it is given up and asked once more: a connection a
/// sleep or a network change left dead. Codex's `stream_idle_timeout_ms`; a model may think
/// for minutes without a byte.
const IDLE: Duration = Duration::from_secs(300);

/// Whether the tokens are refreshed when they are not about to lapse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Renew {
    /// Only when they are about to lapse.
    IfStale,
    /// Now: the backend refused them.
    Now,
}

/// The tokens in use, and whether a thread is refreshing them.
struct Held {
    tokens: Tokens,
    refreshing: bool,
}

/// OpenAI's models over the ChatGPT backend, billed to the plan signed in with.
pub struct ChatModel {
    http: Box<dyn Http>,
    /// The token endpoint, on a client of its own: a refresh is short.
    issuer: Box<dyn Http>,
    keep: Box<dyn Keep>,
    app: App,
    selection: Selection,
    tokens: Mutex<Held>,
    /// Rung when a refresh ends.
    renewed: Condvar,
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
            issuer,
            keep,
            app,
            selection,
            tokens: Mutex::new(Held {
                tokens,
                refreshing: false,
            }),
            renewed: Condvar::new(),
            texts: Texts::Functions,
            routes: route::Routes::default(),
            pause: PAUSE,
        }
    }

    /// The tokens to send, refreshed first as `renew` says, by `deadline`. One thread refreshes
    /// while the others wait on the condvar: the lock is never held across the request (R6), and
    /// no two refresh at once (R7).
    fn tokens(&self, renew: Renew, deadline: Option<Instant>) -> Result<Tokens, ChatError> {
        let mut state = self.tokens.lock().unwrap_or_else(|p| p.into_inner());
        let mut waited = false;
        while state.refreshing {
            state = self.renewed.wait(state).unwrap_or_else(|p| p.into_inner());
            waited = true;
        }
        let now = tokens::now();
        // A refresh that ended while this thread waited did what it came to do.
        if (waited || renew == Renew::IfStale) && !state.tokens.stale(now) {
            return Ok(state.tokens.clone());
        }
        state.refreshing = true;
        let held = state.tokens.clone();
        drop(state);
        let outcome = self.renew(&held, now, deadline);
        let mut state = self.tokens.lock().unwrap_or_else(|p| p.into_inner());
        state.refreshing = false;
        if let Ok(fresh) = &outcome {
            state.tokens.clone_from(fresh);
        }
        drop(state);
        self.renewed.notify_all();
        outcome
    }

    /// Fresh tokens for `held`: another llx's, when it refreshed first (ours were spent), else
    /// the issuer's, kept before they are used.
    fn renew(
        &self,
        held: &Tokens,
        now: u64,
        deadline: Option<Instant>,
    ) -> Result<Tokens, ChatError> {
        if let Some(kept) = self
            .keep
            .load()
            .filter(|kept| kept != held && !kept.stale(now))
        {
            return Ok(kept);
        }
        let fresh = auth::refresh(self.issuer.as_ref(), held, now, deadline)
            .map_err(ChatError::Unavailable)?;
        self.keep.save(&fresh);
        Ok(fresh)
    }
}

impl Chat for ChatModel {
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        let session = route::session(request.conversation);
        let task = (session.clone(), request.task);
        let offer = offer::Offer::new(request.tools, self.texts, request.conversation);
        let selection = self.selection.or_at(request.effort);
        let body = body(&selection, request, &offer, &session).to_string();
        let address = format!("{BASE}/responses");
        let started = Instant::now();
        let mut renewed = false;
        // A stream cut or timed out may have been billed, so it is asked once more at most:
        // nothing of it was acted on, and the backend stores nothing of a request.
        retried(self.pause, request.deadline, Resend::Once, || {
            let tokens = match self.tokens(Renew::IfStale, request.deadline) {
                Ok(tokens) => tokens,
                Err(error) => return Attempt::Done(Err(error)),
            };
            let bearer = format!("Bearer {}", tokens.access);
            let mut headers = vec![
                ("Authorization", bearer.as_str()),
                ("chatgpt-account-id", tokens.account.as_str()),
                ("originator", self.app.slug),
                ("session-id", session.as_str()),
                ("Content-Type", "application/json"),
                ("Accept", "text/event-stream"),
            ];
            let routed = self.routes.token(&task);
            if let Some(token) = &routed {
                headers.push((route::HEADER, token));
            }
            let reply = match self.http.post(&address, &headers, &body, request.deadline) {
                Ok(reply) => reply,
                Err(HttpError::Unsent(why)) => {
                    return Attempt::Again(ChatError::Unavailable(format!(
                        "ChatGPT is unreachable: {why}"
                    )));
                }
                Err(HttpError::Unanswered(why)) => {
                    let error = ChatError::Unavailable(format!("ChatGPT did not answer: {why}"));
                    return Attempt::Unanswered(error);
                }
            };
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
                401 if !renewed => {
                    renewed = true;
                    match self.tokens(Renew::Now, request.deadline) {
                        Ok(_) => Attempt::Again(ChatError::Unavailable(refusal(401, &reply.body))),
                        Err(error) => Attempt::Done(Err(error)),
                    }
                }
                // A 429 here is the plan's usage limit, hours away: not worth a second try.
                status if TRANSIENT.contains(&status) => {
                    Attempt::Again(ChatError::Unavailable(refusal(status, &reply.body)))
                }
                status => Attempt::Done(Err(ChatError::Unavailable(refusal(status, &reply.body)))),
            }
        })
    }
}

/// The completion in a streamed reply: its finished output items and the usage it ended with.
pub(crate) fn read(body: &str, model: &str) -> Result<Completion, ChatError> {
    let mut completion = Completion::default();
    let mut said: Vec<String> = vec![];
    let mut ended = false;
    // Each item moved, not copied, into what goes back: `json!` of a value serializes a second
    // tree of it, and a sealed reasoning is kilobytes.
    let carry = |item: Value| {
        let mut kept = json!({"model": model});
        kept["item"] = item;
        kept
    };
    for mut event in sse::events(body) {
        match event["type"].as_str().unwrap_or("") {
            "response.output_item.done" => {
                let mut item = event["item"].take();
                match item["type"].as_str() {
                    Some("message") => {
                        said.extend(
                            item["content"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter(|part| part["type"] == "output_text")
                                .filter_map(|part| part["text"].as_str().map(str::to_string)),
                        );
                        // Its phase goes back with it: a note beside calls is not an answer.
                        // Without its id: nothing is stored at the backend to name.
                        if item["phase"].is_string() {
                            if let Some(fields) = item.as_object_mut() {
                                fields.remove("id");
                            }
                            completion.carried.push(carry(item));
                        }
                    }
                    Some("function_call") => completion.calls.extend(call(&item)),
                    // A tool of free text offered as a custom one: its input is the text whole.
                    Some("custom_tool_call") => completion.calls.extend(custom_call(&item)),
                    // Nothing is stored at the backend: only a sealed reasoning can go back, and
                    // only to the model that sealed it.
                    Some("reasoning") if item["encrypted_content"].is_string() => {
                        completion.carried.push(carry(item));
                    }
                    _ => {}
                }
            }
            "response.completed" => {
                completion.usage = usage(&event["response"], model);
                ended = true;
            }
            // Cut at the output limit, the reply so far stands and the agent goes on; refused by
            // the content filter, there is nothing to go on from.
            "response.incomplete" => {
                if event.pointer("/response/incomplete_details/reason")
                    == Some(&json!("content_filter"))
                {
                    return Err(ChatError::Invalid(
                        "ChatGPT withheld the reply (content filter)".into(),
                    ));
                }
                completion.usage = usage(&event["response"], model);
                completion.ending = Ending::Cut;
                ended = true;
            }
            "response.failed" | "error" => {
                let error = event.pointer("/response/error").unwrap_or(&event);
                let message = error["message"].as_str().unwrap_or("no message");
                return Err(ChatError::Unavailable(format!("ChatGPT failed: {message}")));
            }
            _ => {}
        }
    }
    if !ended {
        return Err(ChatError::Invalid(
            "the reply ended before it was complete".into(),
        ));
    }
    let said = said.join("\n");
    completion.text = Some(said.trim().to_string()).filter(|text| !text.is_empty());
    Ok(completion)
}

fn call(item: &Value) -> Option<ToolCall> {
    let raw = item["arguments"].as_str().unwrap_or("{}");
    Some(ToolCall {
        id: item["call_id"].as_str()?.to_string(),
        name: item["name"].as_str()?.to_string(),
        // Arguments that are not JSON stay as their text: the agent tells the model they were
        // incomplete instead of running the tool with none.
        arguments: serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string())),
    })
}

/// What the reply took. A ChatGPT plan is paid by subscription: no cost per call.
fn usage(response: &Value, model: &str) -> Usage {
    let tokens = |pointer: &str| {
        response
            .pointer(pointer)
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    Usage {
        model: response["model"].as_str().unwrap_or(model).to_string(),
        cost: 0.0,
        millis: 0,
        input_tokens: tokens("/usage/input_tokens"),
        cached_tokens: tokens("/usage/input_tokens_details/cached_tokens"),
        output_tokens: tokens("/usage/output_tokens"),
    }
}

/// Why the backend refused, in words: its own message where it sent one.
fn refusal(status: u16, body: &str) -> String {
    let value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let message = value
        .pointer("/error/message")
        .or_else(|| value.get("detail"))
        .and_then(Value::as_str)
        .map_or_else(|| body.chars().take(300).collect(), str::to_string);
    match status {
        429 => format!("the ChatGPT plan's usage limit is reached: {message}"),
        401 | 403 => format!("ChatGPT refused the sign-in ({status}): {message}; /login again"),
        _ => format!("ChatGPT answered {status}: {message}"),
    }
}

#[cfg(test)]
#[path = "responses_tests.rs"]
mod tests;
