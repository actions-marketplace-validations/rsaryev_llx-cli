//! The chat model a session thinks with, switchable between tasks: `/login` puts in the one it
//! chose without rebuilding the agent, its tools or its conversation.

use std::sync::RwLock;

use contracts::chat::{Chat, ChatError, Completion, Request};

use crate::choice::Choice;
use crate::providers::{self, Provider};

pub(crate) struct LiveChat(RwLock<Box<dyn Chat>>);

impl LiveChat {
    /// The model of `choice` (one that asks to sign in, when there is no sign-in yet), its keys
    /// read in `vars`.
    pub(crate) fn of(choice: &Choice, vars: providers::Vars) -> Self {
        let chat = choice
            .provider
            .chat_in(&choice.selection, vars)
            .unwrap_or_else(|| Box::new(SignedOut(choice.provider.clone())));
        Self::new(chat)
    }

    pub(crate) fn new(chat: Box<dyn Chat>) -> Self {
        Self(RwLock::new(chat))
    }

    /// Think with `chat` from the next request on.
    pub(crate) fn set(&self, chat: Box<dyn Chat>) {
        *self.0.write().unwrap_or_else(|poison| poison.into_inner()) = chat;
    }
}

impl Chat for LiveChat {
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        let chat = self.0.read().unwrap_or_else(|poison| poison.into_inner());
        chat.complete(request)
    }
}

/// The chat model of a session not signed in yet: every request says how to sign in.
pub(crate) struct SignedOut(pub Provider);

impl Chat for SignedOut {
    fn complete(&self, _: Request) -> Result<Completion, ChatError> {
        Err(ChatError::Unavailable(format!(
            "not signed in to {}: type /login",
            self.0.name()
        )))
    }
}

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;
