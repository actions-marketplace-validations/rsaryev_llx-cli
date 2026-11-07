//! Test-only: an `Http` that answers from a script and keeps what it was sent, and the pieces a
//! request is built from.

use std::fmt::Write as _;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use contracts::catalog::{Effort, ModelEntry, Selection};
use provider_kit::http::{Http, HttpError, Reply};
use serde_json::Value;

/// What one request carried: its address, its headers and its body.
pub(crate) struct Sent {
    pub(crate) address: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Value,
}

/// An `Http` that gives each request the next of its answers, and keeps every request.
pub(crate) struct Scripted {
    answers: Mutex<Vec<Result<Reply, HttpError>>>,
    pub(crate) sent: Arc<Mutex<Vec<Sent>>>,
}

impl Http for Scripted {
    #[expect(
        clippy::unwrap_in_result,
        reason = "a test's fake: a poisoned lock fails the test"
    )]
    fn post(
        &self,
        address: &str,
        headers: &[(&str, &str)],
        body: &str,
        _: Option<Instant>,
    ) -> Result<Reply, HttpError> {
        self.sent.lock().unwrap().push(Sent {
            address: address.into(),
            headers: (headers.iter())
                .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
                .collect(),
            body: serde_json::from_str(body).unwrap_or(Value::Null),
        });
        let next = self.answers.lock().unwrap().pop();
        next.unwrap_or_else(|| Err(HttpError::Unanswered("nothing scripted".into())))
    }
}

/// A scripted `Http` answering `answers` (a status and a body each) in order; what it was sent.
pub(crate) fn scripted(answers: Vec<(u16, String)>) -> (Box<dyn Http>, Arc<Mutex<Vec<Sent>>>) {
    let answers = (answers.into_iter().rev())
        .map(|(status, body)| {
            Ok(Reply {
                status,
                headers: vec![],
                body,
            })
        })
        .collect();
    let sent = Arc::new(Mutex::new(vec![]));
    let http = Scripted {
        answers: Mutex::new(answers),
        sent: Arc::clone(&sent),
    };
    (Box::new(http), sent)
}

/// A model that takes `efforts`, at `effort`.
pub(crate) fn selection(id: &str, efforts: Vec<Effort>, effort: Effort) -> Selection {
    let entry = ModelEntry {
        id: id.into(),
        name: id.into(),
        efforts,
        default: effort,
        context: 1_000_000,
        output: 128_000,
        marks: true,
    };
    Selection::new(entry)
}

/// A stream of `events`, as the API sends it.
pub(crate) fn stream(events: &[Value]) -> String {
    let mut body = String::new();
    for event in events {
        let kind = event["type"].as_str().unwrap_or("event");
        let _ = write!(body, "event: {kind}\ndata: {event}\n\n");
    }
    body
}
