//! The mode a run works in: a name, a line on what it does, its rules, and what its commands may
//! reach (ARCHITECTURE.md K5). The gate judges each effect by the mode's rules and the command
//! line's; then the reach lets the rest run in its sandbox (B3): the workspace's, which changes
//! the workspace alone, or the read-only one; what must leave the sandbox (B6), or what a mode
//! that reaches anything does, it asks the person, or refuses with no one to ask. The context
//! says the same rules, from the same values, so the two cannot drift apart; a mode never filters
//! the tools (the list stays the same, and so does the prompt cache). A change in a session
//! reaches the model as a `<mode_change>` note.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use contracts::gate::{Effect, Gate, Needs, Verdict};

use crate::gate::{Asker, declined};
use crate::rules::{Rules, Ruling};
use crate::sync::lock;
use protocol::Approval;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Mode(Arc<Named>);

#[derive(Debug, PartialEq, Eq)]
struct Named {
    name: String,
    hint: String,
    rules: Rules,
    reach: Reach,
}

/// What a mode's commands may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reach {
    /// What the person allows (the rules, or their answer).
    Anything,
    /// The workspace's files: they run in its sandbox, and its files change, without asking.
    Workspace,
    /// Read, and change nothing: they run in the sandbox, without asking.
    ReadOnly,
}

impl Mode {
    pub(crate) fn new(name: &str, hint: &str, rules: Rules, reach: Reach) -> Self {
        Self(Arc::new(Named {
            name: name.to_string(),
            hint: hint.to_string(),
            rules,
            reach,
        }))
    }

    pub(crate) fn reach(&self) -> Reach {
        self.0.reach
    }

    pub(crate) fn name(&self) -> &str {
        &self.0.name
    }

    /// One line on what the mode does.
    pub(crate) fn hint(&self) -> &str {
        &self.0.hint
    }

    pub(crate) fn rules(&self) -> &Rules {
        &self.0.rules
    }
}

/// The mode now, read by the gate and the person on whichever thread asks (an agent's, a call's
/// running beside others) and set between tasks.
#[derive(Debug)]
pub(crate) struct ModeCell(Mutex<Mode>);

impl ModeCell {
    pub(crate) fn new(mode: Mode) -> Self {
        Self(Mutex::new(mode))
    }

    pub(crate) fn get(&self) -> Mode {
        lock(&self.0).clone()
    }

    pub(crate) fn set(&self, mode: Mode) {
        *lock(&self.0) = mode;
    }
}

/// `/mode`: each mode with what it does, the current one marked.
pub(crate) fn list(current: &Mode, modes: &[Mode]) -> String {
    let line = |mode: &Mode| {
        let mark = if mode.name() == current.name() {
            "*"
        } else {
            " "
        };
        format!("{mark} {:<11} {}\n", mode.name(), mode.hint())
    };
    modes.iter().map(line).collect()
}

/// What the person is told when the mode changes.
pub(crate) fn switched(mode: &Mode) -> String {
    format!("Mode: {}. {}\n", mode.name(), mode.hint())
}

/// Why an effect no rule allows is refused with no one to approve it.
const NO_ONE: &str = "no rule allows it, and no one is at a terminal to approve it: the person \
can rerun llx with --allow and a rule that names it, a --mode that allows it, or --yes (-y)";

/// Why a read-only mode refuses what must leave its sandbox.
const SANDBOXED: &str = "the mode is read-only: nothing runs outside its sandbox";

/// Why an effect is refused when the terminal gave no answer.
const NO_ANSWER: &str = "no answer came from the terminal";

/// The mode as the agent's context states it, its rules the command line's too (`Modes::with`).
/// `terminal`: someone can approve at the terminal. `asks`: the `ask` tool is offered.
pub(crate) fn describe(mode: &Mode, terminal: bool, asks: bool) -> String {
    let mut text = format!("{}: {}", mode.name(), mode.hint());
    let rules = mode.rules();
    if let Some(said) = rules.describe() {
        text.push(' ');
        text.push_str(&said);
    }
    match mode.reach() {
        Reach::ReadOnly => {
            text.push_str(
                " Commands run without asking, in a sandbox: they read every file, but writing a \
                 file, opening a socket or reaching another program fails; $TMPDIR is a scratch \
                 directory of their own.",
            );
            return text;
        }
        Reach::Workspace => {
            text.push_str(
                " Commands run without asking, in a sandbox: they read every file and write \
                 inside the workspace (never its .git; they only read it where it holds the home \
                 directory, or on Linux where it has a .git) and in $TMPDIR, a scratch directory of \
                 their own; writing elsewhere, opening a socket or reaching another program \
                 fails. Files inside the workspace change without asking.",
            );
            text.push_str(match terminal {
                true => {
                    " A command that must reach further sets `escalate`, and is asked of the \
                     person, as is a change inside .git; they may refuse it, and a refusal says why."
                }
                false => {
                    " A command with `escalate`, or a change inside .git, is refused: no one is at \
                     a terminal to approve it."
                }
            });
        }
        // With every tool allowed whole there is nothing else to ask or refuse.
        Reach::Anything if rules.allows_every_tool() => {}
        Reach::Anything => text.push_str(match terminal {
            true => " Anything else is asked of the person, who may refuse it; a refusal says why.",
            false => {
                " Anything else that changes something (a command, a file) is refused: no one \
                 is at a terminal to approve it. Reading files works."
            }
        }),
    }
    if asks && terminal {
        text.push_str(
            " Ask the person with `ask` only when a decision is theirs and the tools cannot \
             settle it; otherwise make a reasonable assumption and state it.",
        );
    }
    text
}

/// The note that tells the model the mode changed mid-session.
pub(crate) fn notice(mode: &Mode, terminal: bool, asks: bool) -> String {
    format!(
        "<mode_change>\nThe mode is now {}\nWhat the context or an earlier note said about the \
         mode no longer holds.\n</mode_change>",
        describe(mode, terminal, asks)
    )
}

/// The gate as the current mode answers it: its rules (the command line's among them) first,
/// then the person. Each mode keeps its own "always" answers, so one given in a mode that asks
/// little does not carry into one meant to ask more.
pub(crate) struct ModeGate<'a> {
    mode: &'a ModeCell,
    /// `None`: nobody to ask.
    asker: Option<Box<dyn Asker + 'a>>,
    /// The tools the person said not to ask about again, by mode.
    always: Mutex<HashMap<String, HashSet<String>>>,
    /// Told how long the person took to answer, so it does not count toward the time limit.
    waited: &'a (dyn Fn(Duration) + Sync),
}

impl<'a> ModeGate<'a> {
    /// Asking `asker`, when there is one.
    pub(crate) fn new(
        mode: &'a ModeCell,
        asker: Option<Box<dyn Asker + 'a>>,
        waited: &'a (dyn Fn(Duration) + Sync),
    ) -> Self {
        Self {
            mode,
            asker,
            always: Mutex::default(),
            waited,
        }
    }
}

impl Gate for ModeGate<'_> {
    fn check(&self, effect: Effect) -> Verdict {
        let mode = self.mode.get();
        let ruling = mode.rules().judge(effect.tool, effect.subject);
        match (ruling, mode.reach(), effect.needs) {
            (Ruling::Denied(why), _, _) => {
                return Verdict::Deny(format!("{why} (mode {})", mode.name()));
            }
            // Nothing to ask: it cannot change anything.
            (Ruling::Allowed | Ruling::Unsettled, Reach::ReadOnly, Needs::Sandbox) => {
                return Verdict::ReadOnly;
            }
            (Ruling::Allowed | Ruling::Unsettled, Reach::ReadOnly, Needs::Open) => {
                return Verdict::Deny(format!("{SANDBOXED} (mode {})", mode.name()));
            }
            (Ruling::Allowed, Reach::Anything | Reach::Workspace, _) => return Verdict::Allow,
            // Nothing to ask: it can change the workspace's files alone.
            (Ruling::Unsettled, Reach::Workspace, Needs::Sandbox) => return Verdict::Workspace,
            (Ruling::Unsettled, Reach::Workspace, Needs::Open)
            | (Ruling::Unsettled, Reach::Anything, _) => {}
        }
        let Some(asker) = &self.asker else {
            return Verdict::Deny(NO_ONE.into());
        };
        // The guard goes with the condition, before the person is asked (R6).
        if (lock(&self.always).get(mode.name())).is_some_and(|tools| tools.contains(effect.tool)) {
            return Verdict::Allow;
        }
        // The asker holds its person for the question (`gate::one_question`).
        let asked = Instant::now();
        let approval = asker.approve(&effect);
        (self.waited)(asked.elapsed());
        match approval {
            Some(Approval::Yes) => Verdict::Allow,
            Some(Approval::Always) => {
                let mut always = lock(&self.always);
                let tools = always.entry(mode.name().to_string()).or_default();
                tools.insert(effect.tool.to_string());
                Verdict::Allow
            }
            Some(Approval::No(why)) => Verdict::Deny(declined(why)),
            None => Verdict::Deny(NO_ANSWER.into()),
        }
    }
}

#[cfg(test)]
#[path = "mode_tests.rs"]
mod tests;
