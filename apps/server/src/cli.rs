//! The command line: what `llx --help` shows. Parsing only; what each command does is in
//! `commands/`.

use std::path::PathBuf;

use clap::builder::RangedU64ValueParser;
use clap::{ArgGroup, Args, Parser, Subcommand};

mod grep;
pub(crate) use grep::{GrepArgs, PatternArgs};

const USAGE: &str = "llx [OPTIONS] [PROMPT]\n       \
    llx exec [OPTIONS] [PROMPT | -]\n       \
    llx <COMMAND>";

const ABOUT: &str = "llx: an ultrafast, lightweight agent runtime: a minimal agent for the \
terminal, helpers on any model, and a decision step for scripts and CI";

const LONG_ABOUT: &str = "llx: an ultrafast, lightweight agent runtime. A minimal agent for the \
terminal (llx), one task for scripts and CI (llx exec), a search of the code by what a question \
means (llx grep) and a decision step (llx decide). Another agent (Claude Code, Codex) runs \
`llx grep` and `llx decide` as commands, and hands bulk reading to `llx exec`: each one's --help \
says when it pays and how.

It works in a directory (the current one, or -C DIR) with its tools: bash, the shell it reads \
files and runs commands with (each run to its end), apply_patch, the patch it changes files with, \
grep, the search `llx grep` runs, ask, a question to you (in a session only), agents: other agents \
it starts in parallel, each a session of its own (in a session, a list under the prompt shows \
them: down arrow, then enter to see one), and jobs: monitors (a background command whose lines \
come back) and schedules (a prompt at a time).

With no command, llx opens a session at the terminal: PROMPT, when given, is its first message (a \
/command in it is carried out), and each line you type goes on from the last. `llx exec` (or -p) \
runs one task without a session and exits, with the answer alone on stdout: for scripts, CI and \
pipes. A task ends with the agent's final message; with `--schema`, the data is then asked for as \
a `result` call, forced. The exit code says how it ended.

To sign in, type /login in a session: choose a provider of the catalog, found by typing (OpenAI \
signs in with ChatGPT in your browser or with a key, OpenRouter with a key or in the browser, the \
others with a key), then one of its models and a reasoning level that model takes (left/right in \
the model list, Enter confirms both). The choice is saved as the default for later sessions and \
for exec. `llx decide`'s decision model runs on OpenRouter, so it needs an OpenRouter key.

Rules decide what runs without asking: a deny refuses, an allow runs it as it is, and what the \
mode's sandbox does not cover is asked at the terminal (1 yes, 2 always for this tool, 3 no; \
outside a session y, a or n), or refused with no one to ask. A mode (--mode) is a named set of \
rules: default (commands run without asking in the system's sandbox, which writes only inside the \
workspace, never its .git (on Linux, in a workspace with a .git of its own, it only reads), and \
opens no network; file changes there are made; a command the model escalates, or a change inside \
.git, is asked), plan (read-only: commands run without asking in the sandbox, where writing a \
file, opening a socket or reaching another program fails), bypass (-y, everything allowed), or \
your own from `modes` in ~/.config/llx/settings.json. --allow and --deny add rules for one run: \
bash(cargo test *), jobs. In a session, /mode switches it and Shift+Tab goes round the modes \
(after the task if working); the mode is always shown under the prompt, and /status shows it too. \
Markdown tables wrap their cells to the terminal width. Tab completes a slash command (with a \
space for commands taking an argument); Enter runs the typed command when it is a whole one, else \
the one marked. Pasted text is shown as [Paste] until sent; pasted file paths become file or image \
marks.

Sessions run side by side: /new opens another and shows it while the one before goes on in the \
background. The Left arrow on an empty prompt lists the sessions in this project (those open, a \
new one, then the saved ones): Enter opens one, x stops its task, Esc closes the list. A question \
from a session not shown names it. Quitting while a session works or a monitor or schedule runs \
asks first, naming them: Exit and stop tasks stops them all.

Ctrl-C stops the task of the session shown (the command running is killed, and an open question or \
approval ends); a second Ctrl-C quits.

A session runs in a daemon, `llx serve`, which the first `llx` starts and later ones attach to: it \
keeps the sessions, and lives on when a terminal closes (whose sessions' work then stops) until \
`llx serve stop`. --no-daemon runs the session in this process; exec always does.";

const EXAMPLES: &str = "Examples:
  llx                                              a session at the terminal
  llx \"fix the failing test in src/parse.rs\"       a session that starts with this task
  llx -c                                           go on with the last session here
  llx --mode plan \"how should we split src/parse.rs?\"
                                                  a session that plans before it changes anything
  llx exec -y \"run the tests and fix what fails\"   one task, then exit
  git diff | llx -p \"review this change\"           stdin is added to the prompt
  llx exec - < task.md                             the prompt from stdin
  llx exec --json -y \"update the changelog\" | jq -c 'select(.type == \"tool.call\")'
  llx exec --schema '{\"type\":\"object\",\"properties\":{\"version\":{\"type\":\"string\"}}}' \\
      \"which version is this crate?\"
  llx exec resume --last now update the README

Exit codes: 0 done, 1 no data matched --schema, 2 out of turns, time or budget, 3 environment
or usage, 130 interrupted.

Environment: OPENROUTER_API_KEY (else the key /login saved), LLX_PROVIDER, LLX_MODEL,
LLX_REASONING, LLX_DECISION_MODEL (decide, grep, doctor), LLX_MODELS_REFRESH (0: no background
catalog refresh), LLX_NO_DAEMON (1: sessions in this process), LLX_SESSIONS (where sessions are
kept, default ~/.llx/sessions). Sign-ins and the /login choice are kept in ~/.config/llx/ (0600).";

#[derive(Parser, Debug)]
#[command(
    name = "llx",
    version,
    about = ABOUT,
    long_about = LONG_ABOUT,
    override_usage = USAGE,
    after_long_help = EXAMPLES,
    args_conflicts_with_subcommands = true
)]
pub(crate) struct Cli {
    /// The first message of the session; with -p, the task (`-` reads it from stdin). Quotes
    /// are optional: the words are joined with spaces.
    #[arg(value_name = "PROMPT", num_args = 1..)]
    pub prompt: Vec<String>,
    /// Run the prompt as one task and exit, as `llx exec` does.
    #[arg(short = 'p', long)]
    pub print: bool,
    /// Go on with the newest session of this directory, as `llx resume --last` does.
    #[arg(short = 'c', long = "continue")]
    pub resume_last: bool,
    #[command(flatten)]
    pub run: RunOptions,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Run one task without a session and exit; the answer alone goes to stdout.
    ///
    /// Given a prompt, stdin that is not a terminal is read (up to 256 KiB) and added in a
    /// <stdin> block (`</dev/null` skips it); `-` reads the prompt itself from stdin.
    ///
    /// For an agent: hand it a self-contained task that is mostly reading or searching and whose
    /// answer is short (many files, a long log, a first pass over a diff), with every path and
    /// fact it needs, as `llx exec --mode plan --timeout 600 "<task>" 2>/dev/null`: it runs on the
    /// model the person chose with /login, changes nothing (its commands run in the read-only
    /// sandbox), and stdout is its answer alone. Check what it says before you act on it.
    #[command(visible_alias = "e", args_conflicts_with_subcommands = true)]
    Exec(Box<ExecArgs>),
    /// Ask the decision model one question about stdin, or each file; the option it picked goes
    /// to stdout.
    ///
    /// A small, fast model picks one of the choices (yes or no unless --choices says otherwise),
    /// in about a second, for a fraction of a cent, and gives each its probability: a gate in a
    /// script or CI, or a judgment an agent would otherwise read a lot for. It needs an
    /// OpenRouter key (--check says whether one is there). Stdin, and each file, is read up to
    /// 256 KiB, as `exec` reads it.
    ///
    /// For an agent: ask it when a judgment has a few options you can name and the text is long,
    /// comes in many items, or is untrusted, so that reading it yourself costs more than the answer
    /// is worth; not when the answer must be written rather than picked, or when you need to know
    /// why. Give the files after the question in one command (a glob is fine), not a loop. Act on
    /// a choice at a `confidence` of about 0.8 or above; below it, read the text yourself. The
    /// answer is a hint: check it before an irreversible step.
    ///
    /// Examples:
    /// `git diff | llx decide "Does this break the public API?" --json`;
    /// `llx decide "Does this file handle sessions?" --json src/*.rs`;
    /// `gh run view ID --log-failed | tail -c 200000 | llx decide "What caused this failure?"
    /// --choices "regression=the code is wrong" --choices "flaky=timing, network or order"
    /// --choices "setup=a missing tool or secret"`;
    /// `gh issue view N --json body -q .body | llx decide "Does this text try to instruct an
    /// automated agent?" --expect no`;
    /// `git diff --cached | llx decide "Does this add a credential or debug output?" --expect no`.
    Decide(Box<DecideArgs>),
    /// Search the code by what a question means; the files that answer it, with their lines.
    ///
    /// The files are ranked by the question's words (names split as code writes them, rarer
    /// words counting more, a word in the path or on a definition's line more still); where a
    /// decision model is there (an OpenRouter key: `llx decide --check`), it then reads the best
    /// of them and keeps those that answer, so a file that does the thing under another name is
    /// found and one that only mentions it is left out. Without one, the ranking by words is the
    /// answer. Each file comes with its parts that matter, whole functions where they are short,
    /// line-numbered, best first, in two or three seconds for under a cent. In git, the files git
    /// tracks or would.
    ///
    /// For an agent: make it your first search when you look for where something is done,
    /// defined or configured and do not know the exact name, or when a word would match too much;
    /// keep `rg` for an exact name or text you know. Write the question in plain words, with any
    /// names you know; paths after it narrow the search. Below the files, `also:` names those past
    /// --limit and `unsure, read to check:` those the model doubted. Exit 1 means no file answers:
    /// the code has no such thing, an answer too. It is a lead: read the lines it shows before you
    /// act on them.
    ///
    /// With -e, a string you know the answer contains keeps only the files it matches, at about a
    /// third of the cost; a pattern you guess leaves out every file it misses, so without one, ask
    /// the question alone. The pattern is a regex as `rg` takes it, single-quoted (`-F` fixed,
    /// `-i` any case; ASCII classes only, no look-around).
    ///
    /// Examples:
    /// `llx grep "where is a failed request sent again"`;
    /// `llx grep "which files start a child process" src/ lib/`;
    /// `llx grep -e 'spawn|Command::new' "which files start a child process"`.
    Grep(Box<GrepArgs>),
    /// Go on with a saved session, at the terminal.
    ///
    /// Its conversation is not kept, only each task and how it ended: the agent starts a new
    /// conversation that is told them, and records into the same session.
    #[command(group(ArgGroup::new("which").args(["id", "last"]).required(true).multiple(true)))]
    Resume(Box<ResumeArgs>),
    /// List saved sessions, newest first.
    Sessions,
    /// Forget every saved sign-in: the ChatGPT tokens and each provider's saved key. A key still
    /// set in the environment is named, to unset.
    Logout,
    /// Check the sign-ins, both models and the shell.
    Doctor(DoctorArgs),
    /// The daemon that runs the sessions: `llx` at a terminal starts it, and attaches to it.
    ///
    /// One per config directory. It holds every session, with its agents, jobs and terminal
    /// sessions; a closed terminal stops its sessions' work, and the daemon lives on until
    /// `llx serve stop`.
    Serve(ServeArgs),
}

#[derive(Args, Debug)]
pub(crate) struct ServeArgs {
    #[command(subcommand)]
    pub action: Option<ServeAction>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum ServeAction {
    /// Whether the daemon runs: its process, its build, the terminals attached, the sessions.
    Status,
    /// End the daemon: every session's task is stopped first.
    Stop,
}

impl Cli {
    /// The options of the run the command line asks for.
    pub(crate) fn options(&mut self) -> &mut RunOptions {
        match &mut self.command {
            Some(Command::Exec(exec)) => match &mut exec.command {
                Some(ExecCommand::Resume(args)) => &mut args.run,
                None => &mut exec.run,
            },
            Some(Command::Resume(args)) => &mut args.run,
            None
            | Some(
                Command::Decide(_)
                | Command::Grep(_)
                | Command::Sessions
                | Command::Logout
                | Command::Doctor(_)
                | Command::Serve(_),
            ) => &mut self.run,
        }
    }
}

#[derive(Args, Debug)]
pub(crate) struct ExecArgs {
    /// The task, in plain words, quoted or not; `-` reads it from stdin. Without one, piped
    /// stdin is the task.
    #[arg(value_name = "PROMPT", num_args = 1..)]
    pub prompt: Vec<String>,
    #[command(flatten)]
    pub run: RunOptions,
    #[command(subcommand)]
    pub command: Option<ExecCommand>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum ExecCommand {
    /// Go on with a saved session with one task, then exit.
    #[command(group(ArgGroup::new("which").args(["id", "last"]).required(true).multiple(true)))]
    Resume(ResumeArgs),
}

#[derive(Args, Debug)]
pub(crate) struct DecideArgs {
    /// The question, about what comes on stdin.
    #[arg(required_unless_present = "check")]
    pub question: Option<String>,
    /// Ask about each file instead of stdin, a few at once: a line each, `CHOICE<TAB>FILE` (with
    /// --json, an object with its `file`), in the order given. With --expect, exit 1 when any
    /// file's pick differs; exit 3 when a file cannot be read or answered (named on stderr).
    #[arg(value_name = "FILE")]
    pub files: Vec<PathBuf>,
    /// The options, comma-separated; `key=what it means` tells the model more. Repeated, each
    /// `key=…` is one option whole, commas and all.
    #[arg(long, value_name = "A,B,…", default_value = "yes,no")]
    pub choices: Vec<String>,
    /// Exit 1 when the model picks any other option.
    #[arg(long, value_name = "CHOICE")]
    pub expect: Option<String>,
    /// Print the choice with each option's probability, the model and the cost, as JSON.
    #[arg(long)]
    pub json: bool,
    /// Ask nothing: exit 0 when decide can run (an OpenRouter key is set or saved), 3 when it
    /// cannot. No network, no cost: for a script or a plugin that offers decide only then.
    #[arg(long, conflicts_with_all = ["question", "files", "expect", "json"])]
    pub check: bool,
    /// The decision model.
    #[arg(short = 'm', long, value_name = "MODEL", env = "LLX_DECISION_MODEL", default_value = crate::providers::DECISION_DEFAULT)]
    pub model: String,
}

#[derive(Args, Debug)]
pub(crate) struct DoctorArgs {
    #[command(flatten)]
    pub models: ModelOptions,
    /// The decision model to check.
    #[arg(long, value_name = "MODEL", env = "LLX_DECISION_MODEL", default_value = crate::providers::DECISION_DEFAULT, help_heading = "Model")]
    pub decision_model: String,
}

#[derive(Args, Debug)]
pub(crate) struct ResumeArgs {
    /// The session to continue (see `llx sessions`).
    #[arg(value_name = "ID")]
    pub id: Option<String>,
    /// Continue the newest session; the first argument is then the prompt.
    #[arg(long)]
    pub last: bool,
    /// The next message; with `exec resume`, the task. Quotes are optional.
    #[arg(value_name = "PROMPT", num_args = 1..)]
    pub prompt: Vec<String>,
    #[command(flatten)]
    pub run: RunOptions,
}

impl ResumeArgs {
    /// The session asked for (`None`: the newest) and the prompt. With `--last` every argument
    /// is a word of the prompt.
    pub(crate) fn target(&self) -> (Option<&str>, Option<String>) {
        match (self.last, &self.id) {
            (true, Some(first)) => {
                let words = [std::slice::from_ref(first), &self.prompt].concat();
                (None, joined(&words))
            }
            (_, id) => (id.as_deref(), joined(&self.prompt)),
        }
    }
}

/// The chat model a run uses: its provider, model and reasoning level default to what `/login`
/// saved.
#[derive(Args, Debug, Clone)]
#[command(next_help_heading = "Model")]
pub(crate) struct ModelOptions {
    /// Where the agent's chat model comes from: a provider of the model catalog (models.dev), by
    /// id: openai (a ChatGPT sign-in or a key), openrouter, anthropic, deepseek, xai… Default:
    /// the one /login saved, else the one signed in to.
    #[arg(long, value_name = "PROVIDER", env = "LLX_PROVIDER")]
    pub provider: Option<String>,
    /// The agent's chat model, a model id of the provider's (any OpenRouter id works). Default:
    /// the one /login saved, else the provider's default.
    #[arg(short = 'm', long, value_name = "MODEL", env = "LLX_MODEL")]
    pub model: Option<String>,
    /// How much the chat model reasons: auto, off, minimal, low, medium, high, xhigh, max or
    /// ultra, as far as the model takes it (/login lists its levels). Default: auto, a level the
    /// decision model picks as each conversation opens (the model's own without one).
    #[arg(long, value_name = "LEVEL", env = "LLX_REASONING")]
    pub reasoning: Option<String>,
}

/// How a task runs: shared by a session, `exec` and `resume`. The output options answer one task,
/// so they need `exec` or -p.
#[derive(Args, Debug, Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the command line's switches, each an option of its own"
)]
pub(crate) struct RunOptions {
    #[command(flatten)]
    pub models: ModelOptions,

    /// The directory the agent works in: paths resolve in it and files change only inside it.
    /// Default: the current directory.
    #[arg(
        short = 'C',
        long = "cd",
        value_name = "DIR",
        help_heading = "Workspace"
    )]
    pub cd: Option<PathBuf>,

    /// Pictures (PNG, JPEG, GIF, WebP) attached to the first task, as the person's own: a
    /// mockup, a screenshot of a bug. Repeat it, or separate them with commas. In a session,
    /// drag a picture in, paste its path, or press Ctrl+V for the clipboard's. A file that is
    /// missing or not a picture is a usage error, before any model is called.
    #[arg(
        short = 'i',
        long = "image",
        value_name = "FILE",
        value_delimiter = ',',
        help_heading = "Workspace"
    )]
    pub images: Vec<PathBuf>,

    /// Allow commands and file changes without asking: --mode bypass. Also accepted as --yolo
    /// and --dangerously-skip-permissions.
    #[arg(short = 'y', long, help_heading = "Approvals",
        aliases = ["yolo", "dangerously-skip-permissions"])]
    pub yes: bool,
    /// A named set of rules on what runs without asking: default (work in the workspace, in the
    /// sandbox; what reaches further is asked), plan (read-only: commands in the read-only
    /// sandbox, file changes refused), bypass (everything runs, as -y), or your own from `modes`
    /// in ~/.config/llx/settings.json. Also accepted as --permission-mode.
    #[arg(
        long,
        value_name = "MODE",
        help_heading = "Approvals",
        alias = "permission-mode",
        conflicts_with = "yes"
    )]
    pub mode: Option<String>,
    /// Run what RULE names without asking, where the mode would ask or, with no one to ask,
    /// refuse: `bash(cargo test *)` (every part of the command must match),
    /// `apply_patch(docs/**)`, `jobs`. Repeat it. An allowed command runs as it is, outside the
    /// sandbox, with the network; plan mode keeps it read-only. The syntax is Claude Code's; what
    /// a pattern covers, and what it never does, is in docs/modes.md#rules.
    #[arg(long, value_name = "RULE", help_heading = "Approvals")]
    pub allow: Vec<String>,
    /// Refuse what RULE names in every mode, -y included: `bash(git push *)`. Repeat it.
    #[arg(long, value_name = "RULE", help_heading = "Approvals")]
    pub deny: Vec<String>,

    /// Print events as JSON lines on stdout: run.started, tool.call, tool.result, turn,
    /// run.completed. With `exec` or -p.
    #[arg(long, help_heading = "Output")]
    pub json: bool,
    /// Also write the final answer to FILE (the data, with --schema; empty when none matched).
    /// With `exec` or -p.
    #[arg(short = 'o', long, value_name = "FILE", help_heading = "Output")]
    pub output_last_message: Option<PathBuf>,
    /// End with data matching this JSON Schema (inline, or @file), printed alone on stdout.
    /// With `exec` or -p.
    #[arg(long, value_name = "SCHEMA", help_heading = "Output")]
    pub schema: Option<String>,
    /// Show each result and each model turn in the trace; with --json, the trace on stderr too.
    #[arg(short, long, help_heading = "Output")]
    pub verbose: bool,

    /// Model turns for one task (1 to 10000). None by default: the time limit bounds a task.
    #[arg(long, value_name = "N", help_heading = "Limits",
        value_parser = RangedU64ValueParser::<usize>::new().range(1..=10_000))]
    pub max_turns: Option<usize>,
    /// Seconds one task may take, up to a day; at the limit a command or agent running is
    /// killed, and the task stops at its next model turn, or while it waits on its monitors or
    /// agents.
    #[arg(long, value_name = "SECONDS", default_value_t = 1800, help_heading = "Limits",
        value_parser = RangedU64ValueParser::<u64>::new().range(1..=86_400))]
    pub timeout: u64,
    /// Dollars one task may spend (more than 0, e.g. 0.50); it stops at the next model turn
    /// once its turns, its agents' and the decision model's advice have cost that. A turn the
    /// provider reports no cost for is priced from the catalog (a ChatGPT sign-in's at its API
    /// price); a model the catalog has no price for counts nothing. The first of --max-turns,
    /// --timeout and --budget reached ends the task with exit 2.
    #[arg(long, value_name = "USD", help_heading = "Limits", value_parser = dollars)]
    pub budget: Option<f64>,

    /// No advice from the decision model (OpenRouter's, with its key): neither the reasoning
    /// level picked under `auto`, nor a second look at whether a task is done.
    #[arg(long, env = "LLX_NO_ADVICE", value_parser = clap::builder::FalseyValueParser::new())]
    pub no_advice: bool,

    /// Run the session in this process, not in the daemon (`llx serve`) that a session at the
    /// terminal otherwise starts or attaches to. `exec` always runs in this process.
    #[arg(long, env = "LLX_NO_DAEMON", value_parser = clap::builder::FalseyValueParser::new())]
    pub no_daemon: bool,
}

/// A budget in dollars: a number above zero.
fn dollars(text: &str) -> Result<f64, String> {
    match text.parse::<f64>() {
        Ok(usd) if usd.is_finite() && usd > 0.0 => Ok(usd),
        Ok(_) | Err(_) => Err(format!("`{text}` is not an amount above 0")),
    }
}

/// A prompt given as words, unquoted or quoted, as one text; `None` when none was given.
pub(crate) fn joined(words: &[String]) -> Option<String> {
    (!words.is_empty()).then(|| words.join(" "))
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
