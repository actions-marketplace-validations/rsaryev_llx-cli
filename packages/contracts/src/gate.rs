//! The gate port: a tool asks before an effect outside the conversation (running a command,
//! changing a file), and whoever composes the tools decides: the person at a terminal, a flag, a
//! policy. A tool knows nothing of who answers.

/// What a tool is about to do, as the person reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effect<'a> {
    /// The tool asking, by its spec name.
    pub tool: &'a str,
    /// The one thing a rule on the tool matches, exactly as the tool will use it: the text it
    /// runs, or the place it changes, relative to where it works. Empty when the call has none.
    pub subject: &'a str,
    /// What it will do, in full and on one line: the command, or the file and its +/- lines.
    /// Whoever answers judges this text alone, so nothing that matters is left out of it.
    pub summary: &'a str,
    /// Where it must run: in its sandbox, or beyond it.
    pub needs: Needs,
}

/// Where an effect must run (ARCHITECTURE.md B6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    /// Wherever the verdict puts it: a command in its sandbox when the verdict confines it.
    Sandbox,
    /// Beyond any sandbox: a command the model said must reach further (the network, a file
    /// outside the workspace), or a change no sandbox lets through (inside `.git`).
    Open,
}

/// The answer to an effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    /// Allowed inside the workspace: a command runs where it can change only the workspace's
    /// files (the tool's sandbox), a file in the workspace is changed.
    Workspace,
    /// Allowed as long as it changes nothing: a command runs where it can only read (the tool's
    /// sandbox), or, where it cannot be confined, not at all.
    ReadOnly,
    /// Not allowed, and why: the tool says so to the model, which can try another way.
    Deny(String),
}

pub trait Gate: Send + Sync {
    fn check(&self, effect: Effect) -> Verdict;
}

/// A gate that allows everything, for tests. The app answers its own gate in every mode, `--yes`
/// included: nothing at run time goes through this one.
pub struct Open;

impl Gate for Open {
    fn check(&self, _: Effect) -> Verdict {
        Verdict::Allow
    }
}
