//! `bash`: a shell command run in the workspace: it reads and searches files, and runs builds,
//! tests and git. The model reads the exit code and both streams; a failing command is an answer,
//! not an error.

use std::time::Duration;

use contracts::gate::{Effect, Gate, Needs};
use contracts::tool::{Input, Tool, ToolError, ToolOutput, ToolSpec};
use serde::Deserialize;
use serde_json::{Value, json};

use super::SHELL;
use super::process::{self, Ending, Finished, Job};
use super::sandbox::{self, Scope};
use crate::workspace::Workspace;

const NAME: &str = "bash";
const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const MAX_TIMEOUT_MS: u64 = 600_000;

/// What the model reads of the tool; `{shell}` is the shell this system runs commands with.
const DESCRIPTION: &str = "Run a shell command (`{shell}`) in the workspace root, or in `workdir`, \
and return its exit code, wall time and output: stdout and stderr together, in the order written, \
as a terminal shows them (no colours, a line redrawn in place at its last state, a line repeated \
shown once with its count). Use it for builds, tests, git, package managers and searching (`rg` \
and `rg --files` where installed, `grep`, `find`), and for reading files (`cat`, `sed -n`, \
`head`). Each call is a fresh shell: `cd` and variables do not carry over, so pass `workdir` or \
chain with `&&`. Input is closed and there is no terminal: never start an interactive program \
(an editor, a pager, a prompt); pass flags like `--yes` or `--no-pager`. The command is killed, \
with everything it started, after `timeout_ms` (default 120000, at most 600000): start a server \
in the background with its output sent to a file. The output of a command that succeeds keeps \
about 80000 characters (some 20000 tokens), so files of up to some 1500 lines in all read whole \
in one command; that of one that fails, about 40000. Past that, the start and the end are kept, \
and in any output, lines over 2000 characters are shortened; the result names a file with the \
whole output and the command that reads the cut part: read that file rather than running the \
command again. A non-zero exit is reported as output, not as a failure. Where the context says \
commands run in a sandbox, one that must write outside the workspace, reach the network or open \
a socket (a server) sets `escalate`: the person is asked to let it run as it is.";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    command: Option<String>,
    timeout_ms: Option<u64>,
    workdir: Option<String>,
    escalate: Option<bool>,
}

pub struct BashTool<'a> {
    pub workspace: &'a Workspace,
    pub gate: &'a dyn Gate,
    /// Environment variables left out of a command's environment: the app's own secrets. Only
    /// left out, not hidden: a command can still read them from the app's own environment (`ps
    /// -E`, `/proc/$PPID/environ`) or wherever the app keeps them, so approving a command is
    /// what keeps them from it. Read at each command: the app's secrets can change meanwhile.
    pub hidden: &'a (dyn Fn() -> Vec<String> + Sync),
    /// Whether the person stopped the task: a running command is killed when it says so.
    pub stop: &'a (dyn Fn() -> bool + Sync),
}

impl Tool for BashTool<'_> {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: NAME.into(),
            description: DESCRIPTION.replace("{shell}", SHELL),
            input: Input::Json(parameters()),
        }
    }

    /// Commands of one turn run at once: a model writes several together
    /// only when none needs another's result (searches, listings, checks).
    fn parallel(&self) -> bool {
        true
    }

    fn call(&self, arguments: &Value) -> Result<ToolOutput, ToolError> {
        // What does not fit is a failed call the model reads.
        let args = Args::deserialize(arguments)
            .map_err(|error| ToolError::Failed(format!("unusable arguments: {error}")))?;
        let command = args.command.as_deref().unwrap_or_default();
        if command.trim().is_empty() {
            return Ok(ToolOutput::from(EMPTY.to_owned()));
        }
        let (dir, place) = self.dir(args.workdir.as_deref())?;
        let mut effect = match &place {
            Some(place) => format!("{}  (in {place})", summary(command)),
            None => summary(command),
        };
        let needs = if args.escalate == Some(true) {
            Needs::Open
        } else {
            Needs::Sandbox
        };
        if needs == Needs::Open {
            effect.push_str("  (outside the sandbox)");
        }
        let scope = sandbox::permit(
            self.gate,
            Effect {
                tool: NAME,
                subject: command,
                summary: &effect,
                needs,
            },
        )
        .map_err(|reason| crate::refused(&reason))?;
        let timeout = args
            .timeout_ms
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .clamp(1, MAX_TIMEOUT_MS);
        let names = (self.hidden)();
        let job = Job {
            command,
            dir: &dir,
            timeout: Duration::from_millis(timeout),
            hidden: &names,
            stop: self.stop,
            scope,
            workspace: self.workspace,
        };
        let finished = process::run(&job)
            .map_err(|error| ToolError::Failed(format!("the shell could not start: {error}")))?;
        let refused = matches!(finished.ending, Ending::Exit(code)
            if scope == Scope::Workspace && sandbox::refused(code, &finished.output));
        let mut report = report(finished, place.as_deref());
        if refused {
            report.push_str(REFUSED);
        }
        Ok(report.into())
    }
}

impl BashTool<'_> {
    /// Where a command runs: the root, or `workdir` in it; and, for a workdir, how the person is
    /// shown it. The person judges where it runs as much as what runs: `rm -rf *` in `~` is not
    /// the same command as in the workspace.
    fn dir(
        &self,
        workdir: Option<&str>,
    ) -> Result<(std::path::PathBuf, Option<String>), ToolError> {
        let workdir = workdir.filter(|dir| !dir.trim().is_empty());
        let dir = match workdir {
            Some(dir) => self.workspace.readable(dir)?,
            None => self.workspace.root().to_path_buf(),
        };
        if !dir.is_dir() {
            return Err(ToolError::Failed(format!(
                "{} is not a directory",
                self.workspace.shown(&dir)
            )));
        }
        let place = workdir.map(|_| self.place(&dir));
        Ok((dir, place))
    }

    /// `dir` as the gate and the report show it: relative inside the workspace, else absolute and
    /// said to be outside, once symlinks are followed (a `docs -> ~` link leads outside).
    fn place(&self, dir: &std::path::Path) -> String {
        let shown = self.workspace.shown(dir);
        let real = crate::workspace::real(dir);
        match real.starts_with(self.workspace.root()) {
            true => shown,
            false if real == dir => format!("{shown}, outside the workspace"),
            false => format!(
                "{shown}, which leads to {}, outside the workspace",
                real.display()
            ),
        }
    }
}

/// What a command the sandbox seems to have refused is told.
const REFUSED: &str = "\n(This looks like the sandbox's refusal: commands here write only inside \
the workspace, never its .git (they only read it where it holds the home directory, or on Linux \
where it has a .git), and open no network. If the command needs more, run it again with \
`escalate`: the person is asked.)";

/// An empty command, as models send one now and then for a pause: answered, not failed, so the
/// turn's later calls still run (Codex runs it as `zsh -lc ''`).
const EMPTY: &str = "Exit code: 0\nOutput:\n(the command was empty: nothing ran)";

/// The arguments of a command run to its end.
fn parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "command": {"type": "string", "minLength": 1, "description": "The command to run."},
            "timeout_ms": {"type": "integer", "minimum": 1, "maximum": MAX_TIMEOUT_MS,
                "description": "Kill it after this many milliseconds (default 120000)."},
            "workdir": {"type": "string",
                "description": "The directory to run it in, relative to the workspace root."},
            "escalate": {"type": "boolean",
                "description": "Run it outside the sandbox, as it is, once the person allows it: \
                    only for a command that must write outside the workspace or reach the network."},
        },
        "required": ["command"],
        "additionalProperties": false,
    })
}

/// The whole command as the gate shows it, on one line: each line break becomes a return sign,
/// and nothing else is left out, so what is approved is what runs.
fn summary(command: &str) -> String {
    command
        .trim_end_matches(['\n', '\r'])
        .replace("\r\n", "\n")
        .replace('\n', " \u{23ce} ")
}

/// The report as Codex writes it: how the command ended, its wall time, its directory when it was
/// given one, how many lines it printed when some were left out, then `Output:` and stdout and
/// stderr as one stream, in the order they were written. Each line is a `Key: value`, so the
/// model reads the code the same way whatever tool it was trained on.
fn report(finished: Finished, place: Option<&str>) -> String {
    let seconds = format!("{:.1}", finished.elapsed.as_secs_f64());
    let mut out = vec![];
    match finished.ending {
        Ending::Exit(code) => out.push(format!("Exit code: {code}")),
        Ending::Signal(signal) => out.push(format!("Killed by signal: {signal}")),
        Ending::TimedOut { killed } => out.push(format!(
            "Timed out after: {seconds} seconds ({}; raise timeout_ms, at most \
             {MAX_TIMEOUT_MS}, or run it in the background)",
            killed_or_not(killed)
        )),
        Ending::Stopped { killed } => {
            out.push(format!(
                "Stopped: by the person ({})",
                killed_or_not(killed)
            ));
        }
    }
    // A timeout's line already says how long it ran.
    if !matches!(finished.ending, Ending::TimedOut { .. }) {
        out.push(format!("Wall time: {seconds} seconds"));
    }
    if let Some(place) = place {
        out.push(format!("Directory: {place}"));
    }
    if let Some(lines) = finished.cut_lines {
        out.push(format!("Total output lines: {lines}"));
    }
    out.push("Output:".into());
    let open = finished.open;
    if !finished.output.is_empty() {
        out.push(finished.output);
    }
    if open {
        out.push(
            "(a background process still holds the output: what it prints later is not shown)"
                .into(),
        );
    }
    out.join("\n")
}

fn killed_or_not(killed: bool) -> &'static str {
    match killed {
        true => "the command and everything it started were killed",
        false => "the command could not be killed, and may still be running",
    }
}

#[cfg(test)]
#[path = "bash_tests.rs"]
mod tests;
