//! A miss of the prompt cache, told from a stream's requests as they come: the session's own
//! model's, or one agent's. A request that sent at least half its input uncached, and enough of
//! it to matter, missed; its cause is what came before it in the same stream.

use contracts::history::Spent;

/// Tokens a request sends again, uncached, at least, to count as a miss.
const MISSED: u64 = 20_000;
/// The prompt cache's shortest lifetime (Anthropic's, five minutes): a stream idle this long
/// finds its cache gone.
pub(super) const LIFETIME_MS: u64 = 5 * 60 * 1000;
/// Why a request missed the prompt cache, as far as its steps tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cause {
    /// The stream's first request: a new conversation, nothing cached yet.
    Cold,
    /// The context was elided before it (or shrank): its earlier messages changed.
    Elided,
    /// Another model answered before it: each model has its own cache.
    Model,
    /// The stream was idle past the cache's lifetime.
    Pause,
    /// Nothing in the steps says.
    Unknown,
}

impl Cause {
    pub(super) const ALL: [Self; 5] = [
        Self::Elided,
        Self::Model,
        Self::Pause,
        Self::Cold,
        Self::Unknown,
    ];

    pub(crate) fn words(self) -> &'static str {
        match self {
            Self::Cold => "a new conversation: nothing cached yet",
            Self::Elided => "after the context was elided",
            Self::Model => "after a change of model",
            Self::Pause => "after a pause past the cache's lifetime",
            Self::Unknown => "cause not known",
        }
    }
}

/// One request that missed the cache.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Miss {
    /// Input tokens it sent uncached.
    pub tokens: u64,
    /// What the request cost in all.
    pub cost: f64,
    pub cause: Cause,
}

/// A stream's last request: the session's own, or one agent's.
#[derive(Debug, Clone, Default)]
struct Last {
    input: u64,
    ended_ms: u64,
    model: String,
    elided: bool,
}

/// Each stream's last request, to tell a miss and its cause.
#[derive(Debug, Default)]
pub(crate) struct Streams {
    last: Vec<(String, Last)>,
}

impl Streams {
    fn entry(&mut self, agent: &str) -> &mut Last {
        let at = match self.last.iter().position(|(name, _)| name == agent) {
            Some(at) => at,
            None => {
                self.last.push((agent.to_string(), Last::default()));
                self.last.len() - 1
            }
        };
        &mut self.last[at].1
    }

    /// `agent`'s context was elided: its next request is expected to miss.
    pub(crate) fn elided(&mut self, agent: &str) {
        self.entry(agent).elided = true;
    }

    /// A model request of `agent`'s, begun at `at_ms` (Unix milliseconds) and taking `ms`: a miss
    /// when at least half its input, and [`MISSED`] tokens, was sent uncached.
    pub(crate) fn request(
        &mut self,
        agent: &str,
        model: &str,
        (at_ms, ms): (u64, u64),
        spent: &Spent,
    ) -> Option<Miss> {
        let next = Last {
            input: spent.input_tokens,
            ended_ms: at_ms + ms,
            model: model.to_string(),
            elided: false,
        };
        let before = std::mem::replace(self.entry(agent), next);
        let uncached = spent.input_tokens.saturating_sub(spent.cached_tokens);
        if uncached < MISSED || uncached * 2 < spent.input_tokens {
            return None;
        }
        let cause = if before.input == 0 {
            Cause::Cold
        } else if before.elided || spent.input_tokens + 2_000 < before.input {
            Cause::Elided
        } else if !model.is_empty() && !before.model.is_empty() && model != before.model {
            Cause::Model
        } else if at_ms.saturating_sub(before.ended_ms) >= LIFETIME_MS {
            Cause::Pause
        } else {
            Cause::Unknown
        };
        Some(Miss {
            tokens: uncached,
            cost: spent.cost,
            cause,
        })
    }
}
