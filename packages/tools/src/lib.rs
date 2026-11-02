//! The tools, one module each. A tool implements the agent's tool protocol
//! (`contracts::tool::Tool`); it knows nothing of the agent or the models. A tool with
//! an effect outside the conversation (a changed file, a command) asks a `contracts::gate::Gate`
//! first.
//! - [`workspace`]: the directory the tools work in: where paths resolve, where files may
//!   change.
//! - [`fs`]: `apply_patch`, and the reader of the pictures a person attaches.
//! - [`shell`]: `bash`: a command run in the workspace, its exit code and its output; the model
//!   reads, searches and runs the checks with it. The app starts its own commands the same way (a
//!   monitor's, a short query).
//! - [`ask`]: questions to the person (`contracts::person::Person`), who answers in the app.

use contracts::tool::ToolError;

pub mod ask;
pub mod fs;
pub mod shell;
pub mod workspace;

#[cfg(test)]
mod testing;

/// A gate's refusal, as the model reads it, so it can try another way.
fn refused(reason: &str) -> ToolError {
    ToolError::Failed(format!("not allowed: {reason}"))
}

/// `mutex`, locked even if a holder panicked: what it guards is plain data, still usable, and a
/// tool fails its call rather than the process.
fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
