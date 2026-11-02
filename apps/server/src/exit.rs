//! Exit codes: the contract scripts, CI jobs and other agents read. Changing one is a breaking
//! change (see CHANGELOG.md).

/// The task ended with its answer (or the reader of stdout went away).
pub(crate) const OK: i32 = 0;
/// The answer is a no: no data matched `--schema`, `decide --expect` was not the choice picked.
pub(crate) const FAILED: i32 = 1;
/// Turns, the time limit or the budget ran out before an answer.
pub(crate) const EXHAUSTED: i32 = 2;
/// The environment or the command line is not ready: no key, no network, a bad argument.
pub(crate) const ENVIRONMENT: i32 = 3;
/// Interrupted: Ctrl-C, SIGTERM or SIGHUP.
pub(crate) const INTERRUPTED: i32 = 130;
