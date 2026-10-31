//! What reaches a run between its turns, from outside its own calls: what the person said while
//! it worked, what the work it started in the background came to. Whoever composes the run fills
//! it; the run reads it before each turn and does not end while something is still to come.

pub trait Inbox: Send + Sync {
    /// What came in since the last look, oldest first: each is said to the model as it is.
    fn take(&self) -> Vec<String>;
    /// Whether the run must not end yet: work it started has not answered.
    fn expecting(&self) -> bool;
    /// Wait until something comes in, nothing more is expected, or the run is stopped; what
    /// came in (nothing in the last two cases).
    fn wait(&self) -> Vec<String>;
}
