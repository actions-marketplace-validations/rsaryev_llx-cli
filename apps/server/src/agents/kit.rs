//! What an agent's tools are built from, and the tools built: the main session's, and each
//! agent's own, on a gate that names it and a stop of its own (the task's, or the person's `x` on
//! its row).

use std::sync::OnceLock;
use std::sync::atomic::AtomicU64;

use contracts::chat::Chat;
use contracts::gate::Gate;
use contracts::history::History;
use contracts::model::Decider;
use contracts::tool::{Tool, ToolError, ToolOutput, ToolSpec};
use serde_json::Value;
use tools::fs::PatchTool;
use tools::shell::BashTool;
use tools::workspace::Workspace;

use crate::grep::tool::GrepTool;
use crate::mode::{Mode, ModeCell};
use crate::output::Output;

/// The main session's parts an agent works with.
pub(crate) struct Kit<'a> {
    pub chat: &'a dyn Chat,
    pub workspace: &'a Workspace,
    /// The main session's gate (the mode, the person, the stop).
    pub gate: &'a dyn Gate,
    /// What commands never see, read at each command.
    pub hidden: &'a (dyn Fn() -> Vec<String> + Sync),
    /// Whether the task was stopped.
    pub stopped: &'a (dyn Fn() -> bool + Sync),
    /// The context with no earlier session: a fresh agent knows nothing of the parent's history.
    pub context: &'a str,
    pub max_turns: usize,
    /// The model's context window, set at each task (the model can change between them).
    pub window: &'a AtomicU64,
    /// How each agent's conversation is prepared for the model.
    pub strategy: &'a dyn agent::history::Strategy,
    pub output: &'a Output,
    pub history: &'a dyn History,
    pub mode: &'a ModeCell,
    /// The mode the context states.
    pub started: Mode,
    pub terminal: bool,
    /// The tools the main session offers, set once its tools are built: an agent is offered the
    /// same, so its requests start as the main session's do and read the provider's prompt cache
    /// from there.
    pub main: OnceLock<Vec<ToolSpec>>,
    /// The main session's latest exchange, which a fork starts from.
    pub forks: super::fork::Forks,
    /// The decision model's advice (C9), when one is reachable: an agent's is a level only.
    pub advice: Option<&'a crate::advice::Advice<'a>>,
    /// The decision model `grep` reads the files with (B8), when one is reachable.
    pub searcher: Option<&'a dyn Decider>,
    /// What the decision model costs, charged to the task.
    pub charge: &'a (dyn Fn(f64) + Sync),
}

/// A session's tools: `bash`, through which the model reads, searches and runs the checks,
/// `apply_patch`, through which it changes files (ARCHITECTURE.md B1), and `grep`, which
/// searches the code by what a question means (B8).
pub(crate) struct Tools<'a> {
    kit: &'a Kit<'a>,
    bash: BashTool<'a>,
    patch: PatchTool<'a>,
    grep: GrepTool<'a>,
}

impl<'a> Tools<'a> {
    pub(crate) fn new(
        kit: &'a Kit<'a>,
        gate: &'a dyn Gate,
        stop: &'a (dyn Fn() -> bool + Sync),
    ) -> Self {
        Self {
            kit,
            bash: BashTool {
                workspace: kit.workspace,
                gate,
                hidden: kit.hidden,
                stop,
            },
            patch: PatchTool {
                workspace: kit.workspace,
                gate,
            },
            grep: GrepTool {
                workspace: kit.workspace,
                decider: kit.searcher,
                charge: kit.charge,
            },
        }
    }

    /// Run `work` with the tools as the agent offers them.
    pub(crate) fn with<T>(&self, work: impl FnOnce(&[&dyn Tool]) -> T) -> T {
        let tools: [&dyn Tool; 3] = [&self.bash, &self.patch, &self.grep];
        work(&tools)
    }

    /// Run `work` with the tools as the main session offers them, in its order: an agent's own
    /// where it has one, the rest withheld. Before the main session's first task, its own alone.
    pub(crate) fn as_main<T>(&self, work: impl FnOnce(&[&dyn Tool]) -> T) -> T {
        let Some(main) = self.kit.main.get() else {
            return self.with(work);
        };
        self.with(|own| {
            // One for each of the main session's tools: those the agent has of its own go unused.
            let withheld: Vec<Withheld> = main.iter().cloned().map(Withheld).collect();
            let tools: Vec<&dyn Tool> = (main.iter().zip(&withheld))
                .map(|(spec, kept)| {
                    let mine = own
                        .iter()
                        .copied()
                        .find(|tool| tool.spec().name == spec.name);
                    mine.unwrap_or(kept as &dyn Tool)
                })
                .collect();
            work(&tools)
        })
    }
}

/// A tool of the main session an agent is offered but may not use (`agents`, `ask`, `jobs`):
/// offered the same, refused when called.
struct Withheld(ToolSpec);

impl Tool for Withheld {
    fn spec(&self) -> ToolSpec {
        self.0.clone()
    }

    fn call(&self, _: &Value) -> Result<ToolOutput, ToolError> {
        Err(ToolError::Failed(format!(
            "`{}` is the main session's alone: an agent works with bash, \
             apply_patch and grep, and decides on its best \
             assumption where it would ask",
            self.0.name
        )))
    }
}
