//! A request asked again when the answer says a moment's wait may mend it: at most [`ATTEMPTS`]
//! times, the pause doubling between them. Which answers those are is the caller's to say
//! ([`TRANSIENT`] for the statuses every host means so); a request that went out and got no whole
//! answer (`HttpError::Unanswered`) may have been acted on and billed, and is sent again only as
//! the caller's [`Resend`] says.

use std::time::{Duration, Instant};

/// Attempts at most, the first among them.
const ATTEMPTS: u32 = 3;
/// The first pause; each next one is twice as long.
pub const PAUSE: Duration = Duration::from_millis(800);
/// Statuses a moment's wait may mend on any host: a timeout, an upstream gone or overloaded.
pub const TRANSIENT: [u16; 6] = [408, 500, 502, 503, 504, 529];

/// What one attempt came to.
pub enum Attempt<T, E> {
    /// The answer, or a failure no wait mends.
    Done(Result<T, E>),
    /// A failure another attempt may mend.
    Again(E),
    /// The request went out and no whole answer came back: sent again as [`Resend`] says.
    Unanswered(E),
}

/// Whether a request that went out unanswered is sent again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resend {
    /// Never: what it cost would be paid twice.
    Never,
    /// Once more at most, as another attempt.
    Once,
}

/// `attempt` until it is done, or its last failure after [`ATTEMPTS`]; `pause` before the second.
/// No pause is slept past `until`: the failure is returned instead.
#[expect(
    clippy::disallowed_methods,
    reason = "a backoff between attempts, which no event ends sooner"
)]
pub fn retried<T, E>(
    pause: Duration,
    until: Option<Instant>,
    resend: Resend,
    mut attempt: impl FnMut() -> Attempt<T, E>,
) -> Result<T, E> {
    let mut wait = pause;
    let mut resend = resend;
    let mut made = 0;
    loop {
        made += 1;
        let error = match (attempt(), resend) {
            (Attempt::Done(result), _) => return result,
            (Attempt::Again(error), _) => error,
            (Attempt::Unanswered(error), Resend::Never) => return Err(error),
            // Sent again as any attempt is, after the pause; the next one is not.
            (Attempt::Unanswered(error), Resend::Once) => {
                resend = Resend::Never;
                error
            }
        };
        if made == ATTEMPTS || until.is_some_and(|at| Instant::now() + wait >= at) {
            return Err(error);
        }
        std::thread::sleep(wait);
        wait = wait.saturating_mul(2);
    }
}

#[cfg(test)]
#[path = "retry_tests.rs"]
mod tests;
