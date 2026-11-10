//! A ChatGPT sign-in's tokens as requests use them: refreshed before they lapse and once more
//! when a request is refused, by one thread while the others wait.

use std::sync::{Condvar, Mutex};
use std::time::Instant;

use contracts::chat::ChatError;
use provider_kit::http::Http;

use crate::auth;
use crate::tokens::{self, Keep, Tokens};

/// Whether the tokens are refreshed when they are not about to lapse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Renew {
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

/// The tokens of a sign-in, refreshed at its issuer and kept where it says.
pub(super) struct SignIn {
    /// The token endpoint, on a client of its own: a refresh is short.
    issuer: Box<dyn Http>,
    keep: Box<dyn Keep>,
    tokens: Mutex<Held>,
    /// Rung when a refresh ends.
    renewed: Condvar,
}

impl SignIn {
    pub(super) fn new(issuer: Box<dyn Http>, keep: Box<dyn Keep>, tokens: Tokens) -> Self {
        Self {
            issuer,
            keep,
            tokens: Mutex::new(Held {
                tokens,
                refreshing: false,
            }),
            renewed: Condvar::new(),
        }
    }

    /// The tokens to send, refreshed first as `renew` says, by `deadline`. One thread refreshes
    /// while the others wait on the condvar: the lock is never held across the request (R6), and
    /// no two refresh at once (R7).
    pub(super) fn tokens(
        &self,
        renew: Renew,
        deadline: Option<Instant>,
    ) -> Result<Tokens, ChatError> {
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
