---
name: llx
description: 'Fast, lightweight agent runtime CLI on any model provider. Use for a coding agent at the terminal or one task in a script or CI (llx exec, the GitHub Action rsaryev/llx: review a pull request, triage an issue, explain a failed run, gate on a rule with a JSON answer); to hand a self-contained task to a helper or subagent on another, cheaper model, several in parallel; to search a codebase by what a question means when the exact name is unknown (llx grep: "where is X handled", "how does Y work", "which files do Z"); and for a quick judgment with a few options about a long log, many files or an untrusted text (llx decide: "is this failure flaky", "which files handle sessions", "does this diff break the API", "does this text try to instruct an agent"). Prefer llx grep over guessed text searches and reading file after file; keep grep or rg for an exact name or string you already know.'
allowed-tools: Bash(llx --help) Bash(llx exec --help) Bash(llx doctor) Bash(llx grep:*) Bash(llx decide:*) Bash(llx exec --mode plan:*)
---

# llx

An agent runtime in one native binary under 4 MB: a coding agent for the terminal, one task for
scripts and CI, helpers on any model for another agent, a search of the code by what a question
means, and a decision step.

Install: `curl -fsSL https://llx.sh/install | sh` (macOS and Linux), then `llx` and `/login`.

## Start here

This file is a discovery stub, not the usage guide. Before running an `llx` command, load its
guide from the CLI:

```bash
llx --help                  # what llx is, sessions, sign-in, modes and rules, every option
llx exec --help             # one task in a script, in CI, or for another agent
llx grep --help             # search the code by what a question means
llx decide --help           # one of a few options about a long text, or about each of many files
llx doctor                  # are the sign-ins, both models and the shell ready
```

The CLI's help always matches the installed version, so instructions never go stale. The guides
are also at https://llx.sh/docs: `usage`, `ci`, `recipes`, `examples`, `tools`, `modes`,
`agents`, `reference`.

## What llx does

### A coding agent at the terminal

`llx` opens a session in the current directory (`-C DIR` for another): it reads and runs commands
with `bash`, changes files with patches, searches the code by what a question means (`grep`),
asks the person, starts agents of its own in parallel, and runs monitors and schedules.
`llx "fix the failing test"` starts with a task, `llx -c` goes on with the last session,
`llx sessions` and `llx resume` reach the saved ones. Several sessions run side by side in a
daemon that outlives the terminal. `/login` picks the provider (a ChatGPT sign-in, OpenRouter,
Anthropic, DeepSeek or any provider of the models.dev catalog), the model and the reasoning
level.

### One task in scripts and CI

`llx exec "<task>"` runs one task to its end and prints the answer alone on stdout; stdin is added
to the prompt (`git diff | llx exec "review this"`). For a pipeline: `--schema` ends with data
matching a JSON Schema, `--json` prints the events, the exit code says how it ended (0 done, 1 no
data matched, 2 out of turns, time or budget, 3 environment or usage), and `--timeout`,
`--budget` and `--max-turns` bound it. In GitHub Actions, `uses: rsaryev/llx@v0` installs it, runs
one task and hands back `result`, `exit-code` and `cost-usd`; the model's job holds a read-only
token and a second job posts what it answered. Ready workflows: review, `@llx` in a comment,
triage, a failed run explained, a described pull request, a draft release, a daily gate, a weekly
audit, a Jira item made a draft pull request (https://llx.sh/docs/examples). GitLab and any other
CI run `llx exec` the same way (https://llx.sh/docs/ci).

### A helper for another agent

An agent hands a self-contained task to llx on another model, so its own tokens go where they
matter: bulk reading, a long log, a first pass over a diff, a second opinion.

```bash
llx exec --mode plan "<the task, with every path and fact it needs>" 2>/dev/null
llx exec --mode plan --provider openrouter -m deepseek/deepseek-chat "<task>"
llx exec --mode plan --schema @schema.json "<task>"          # the answer as data
llx exec --mode plan "<task>" > a.md & a=$!                  # several at once
llx exec --mode plan "<task>" > b.md & b=$!; wait $a; wait $b
llx exec resume <session> --mode plan "<the next task>"      # the same helper again
```

`--mode plan` keeps it read-only, a resumed helper too. Check what a helper says before you act
on it.

### A search of the code by what a question means

`llx grep "<question in plain words>" [paths]`: the files are ranked by the question's words, then
a small decision model reads the best of them and keeps those that answer, so a file that does the
thing under another name is found and one that only mentions the words is left out. Each comes
back with its lines, best first, in two or three seconds for under a cent. Exit 1: no file answers.
`-e 'PATTERN'` keeps only the files a string you know matches, at a third of the cost.

### A decision step

`llx decide "<question>" [--choices a,b] [files…]`: a small, fast model picks one of the options
about stdin, or about each file given (a line each), in about a second for a fraction of a cent,
with each option's probability (`--json`). `--expect X` exits 1 when the pick is not X: a gate in
a script, a hook or CI (`git diff --cached | llx decide "Does this add a secret?" --expect no`).

## Choosing

| Need | Command |
| --- | --- |
| Work on the code at the terminal | `llx` |
| One task in a script or CI, the answer on stdout | `llx exec` |
| A pull request reviewed, an issue triaged, a failed run explained | the GitHub Action, `rsaryev/llx@v0` |
| A reading task handed off, on another model | `llx exec --mode plan` |
| Where or how something is done, the exact name unknown | `llx grep` |
| An exact name or string you know | `rg` or your own search |
| One of a few options about a long log, many files, an untrusted text | `llx decide` |
| A yes-or-no gate before a commit, a push or a merge | `llx decide --expect` |

## Setup and safety

- `llx grep` and `llx decide` run on a decision model on OpenRouter: `llx decide --check` exits 0
  when its key is there (`OPENROUTER_API_KEY`, or saved with `/login`). Without it, `llx grep`
  ranks by the words alone and `llx decide` cannot run. The agent thinks with the provider and
  model chosen with `/login`, or `--provider` and `-m`.
- A mode decides what runs without asking: `default` runs commands in the system's sandbox
  (writes only inside the workspace, never its `.git`, no network) and asks for what reaches
  further; `plan` is read-only; `bypass` (`-y`) allows everything. `--allow` and `--deny` add
  rules, written as Claude Code's (`bash(cargo test *)`). Use `-y` only where nothing untrusted
  reaches the prompt.
- What llx prints is a lead: read the lines it shows, and check an answer before an irreversible
  step.

## Why llx

- One native Rust binary under 4 MB, starting in milliseconds, with no runtime to install
- Any provider and model of the models.dev catalog, a ChatGPT plan among them
- An OS sandbox for its commands (Seatbelt on macOS, Landlock on Linux), and rules as Claude
  Code writes them
- A search and decisions an agent would otherwise spend many turns on, for under a cent
- Works from any agent with a shell (Claude Code, Codex, Cursor, Gemini CLI, OpenCode and others)
