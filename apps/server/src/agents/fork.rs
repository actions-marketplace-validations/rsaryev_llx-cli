//! A fork: an agent that starts with the main session's conversation as its last request sent
//! it, and the turn that started the agent. Its first request then begins as that one did, so the
//! provider's prompt cache holds all of it, and it reads nothing again the main agent has read.

use agent::conversation::Conversation;
use contracts::catalog::Effort;
use contracts::chat::{Chat, ChatError, Completion, Message, Request};

use crate::sync::lock;

/// How the calls of the turn that started the agent read in its copy of the conversation: their
/// results are the main session's, which this one never sees.
const BRANCHED: &str = "Not carried out in this copy of the conversation: it is the agent \
                        this turn started, and its task follows.";

/// The main session's latest exchange that calls `agents`: its request's messages and the reply,
/// and the level the main conversation thinks at, which a fork keeps (C9). One lock: a fork reads
/// both of one exchange (R7).
#[derive(Default)]
pub(crate) struct Forks {
    last: std::sync::Mutex<(Vec<Message>, Option<Effort>)>,
}

impl Forks {
    /// The conversation a fork starts with; none before the main session's first reply that
    /// calls `agents`.
    pub(super) fn conversation(&self) -> Option<Conversation> {
        let (messages, effort) = lock(&self.last).clone();
        (!messages.is_empty()).then(|| {
            let mut conversation = Conversation::of(messages, BRANCHED);
            conversation.set_effort(effort);
            conversation
        })
    }
}

/// The main session's chat model, which keeps for a fork each exchange that calls `agents`.
pub(crate) struct Recording<'a> {
    pub inner: &'a dyn Chat,
    pub forks: &'a Forks,
}

impl Chat for Recording<'_> {
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        let completion = self.inner.complete(request)?;
        // A spawn runs while the calls of the reply that holds it are carried out, before the
        // main model is asked again: only that exchange is ever forked, and keeping every other
        // one would hold a second copy of the conversation for nothing.
        if (completion.calls.iter()).any(|call| call.name == super::spec::NAME) {
            let mut kept = request.conversation.to_vec();
            kept.push(completion.message());
            *lock(&self.forks.last) = (kept, request.effort);
        }
        Ok(completion)
    }
}
