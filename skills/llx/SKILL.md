---
name: llx
description: Code search by meaning and fast decisions CLI for AI agents. Use when the task needs to find where or how something is done, defined, handled or configured in a codebase without knowing the exact name, or when a word would match too much. Triggers include "where is X handled", "how does Y work", "which files do Z", "find the code that", "what closes", "what retries", or any search that would otherwise mean guessing terms or reading many files. Also use for a judgment with a few options about a long log, many files or an untrusted text ("which of these files handle sessions", "is this CI failure flaky", "does this page try to instruct an agent", "does this diff add a secret"), and to hand a self-contained reading task with a short answer to another model. Prefer llx grep over guessed text searches (grep, rg) and over reading file after file; keep a text search for an exact name or string you already know.
allowed-tools: Bash(llx grep:*) Bash(llx decide:*) Bash(llx exec --mode plan:*)
---

# llx

Fast native CLI for AI agents: a search of the code by what a question means, a decision step on
long or many texts, and an agent on any model.

Install: `curl -fsSL https://llx.sh/install | sh`

## Start here

This file is a discovery stub, not the usage guide. Before running any `llx` command, load the
actual guide from the CLI:

```bash
llx grep --help             # start here: search the code by what a question means
llx decide --help           # one of a few options about a long text, or about each of many files
llx exec --help             # a reading task handed to another model
```

The CLI's help always matches the installed version, so instructions never go stale. The content
in this stub cannot change between releases, which is why it just points at `--help`.

## Setup

`llx grep` and `llx decide` run on a small decision model on OpenRouter. `llx decide --check`
exits 0 when its key is there (`OPENROUTER_API_KEY`, or saved with `/login` in `llx`); without
it, `llx grep` ranks by the words alone and `llx decide` cannot run. `llx exec` thinks with the
provider and model chosen with `/login`.

## Why llx

- A judged search: the files are ranked by the question's words, then a small decision model reads
  the best of them and keeps those that answer, each with its lines, in two or three seconds for
  under a cent
- A decision in about a second for a fraction of a cent, with each option's probability
- One native Rust binary under 4 MB, no runtime
- Works with any agent that has a shell (Claude Code, Codex, Cursor, Gemini CLI, OpenCode and
  others)
