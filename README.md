<p align="center">
  <img src="site/public/logo.svg" width="72" height="72" alt="llx">
</p>

<h1 align="center">llx</h1>

<p align="center">
  <strong>An ultrafast, lightweight agent runtime for coding and CI</strong>
</p>

<p align="center">
  <code>llx exec</code> runs one task and ends with an answer and an exit code your pipeline can branch on. One binary under 4 MB, on 1,000+ models from 200+ providers: a ChatGPT sign-in, OpenRouter or your own key. A minimal agent at your terminal, too.
</p>

<p align="center">
  <a href="https://github.com/rsaryev/llx/actions/workflows/ci.yml"><img src="https://github.com/rsaryev/llx/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI status"></a>
  <a href="https://github.com/rsaryev/llx/releases/latest"><img src="https://img.shields.io/github/v/release/rsaryev/llx?color=black&label=release" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-black" alt="MIT License"></a>
  <img src="https://img.shields.io/badge/macOS%20%C2%B7%20Linux-black" alt="macOS and Linux">
  <img src="https://img.shields.io/badge/binary-under%204%20MB-black" alt="One binary under 4 MB">
  <img src="https://img.shields.io/badge/Rust-black?logo=rust" alt="Written in Rust">
</p>

<p align="center">
  <a href="#try-it">Try it</a> ·
  <a href="docs/usage.md">Documentation</a> ·
  <a href="#in-ci-and-scripts">In CI</a> ·
  <a href="https://llx.sh">llx.sh</a>
</p>

<p align="center">
  <a href="https://llx.sh">
    <img src="docs/assets/demo.gif" alt="llx at a terminal, as on llx.sh: in the default mode it fixes the failing median test in stats.py without a question (12 seconds, $0.0012 on GPT-6 Luna), then asks how a tie should be decided, each option's code beside it, and builds the choice; then, from the shell as CI calls it, llx exec --mode plan --schema reads the diff and answers with data" width="880">
  </a>
</p>

llx is a model in a loop over five tools, on any provider: `bash`, `apply_patch`, `ask`, `agents` and `jobs`. A task ends when the model replies with a message and no tool call: that message is the answer. With `--schema`, llx then asks the model once more for data matching your JSON Schema. One binary, three uses:

| Who calls | Command | What they get |
| --- | --- | --- |
| **Your CI and scripts** | `llx exec --json --schema …` | One task, then exit: the answer in the shape you define, JSON events, an exit code to branch on |
| **You** | `llx` | A minimal agent at your terminal |
| **Your other agents** | `llx exec` · `llx grep` · `llx decide` | Commands they run from their shell: a helper on any model, a search by meaning, a quick decision. They extend the agent you use, never replace it |

By default llx works in your project without asking: commands run in the system's sandbox, which writes only inside the workspace (never its `.git`; on Linux, in a workspace with a `.git` of its own, they only read) and opens no network, and file changes there are made; what must reach further is asked first, and refused where no one is there to ask. `plan` runs read-only in the same sandbox, `bypass` (`-y`) allows everything, and rules or a mode of your own say the rest ([modes](docs/modes.md)). A task is bounded by `--max-turns`, `--timeout` and `--budget`: the first reached ends it.

It runs on 200+ model providers: a ChatGPT sign-in, OpenRouter, or a key ([models](MODELS.md)). With an OpenRouter key, a small decision model advises each task at its edges: it picks the reasoning level as a conversation opens (`--reasoning auto`, the default), and takes a second look at an answer that may leave work undone (`--no-advice` turns it off).

Every run is saved as a session: `llx -c` goes on with the newest one in this directory, `llx sessions` lists them and `llx resume ID` opens one; `/new` opens another beside it ([sessions](docs/usage.md#sessions-and-the-daemon)).

## Try it

```bash
curl -fsSL https://llx.sh/install | sh
cd your-project && llx                       # a session: /login first, then a task
echo '{"type":"object","properties":{"bump":{"enum":["patch","minor","major"]},"ship":{"type":"boolean"}},"required":["bump","ship"]}' > ship.json
git diff main... | llx exec --json --mode plan --schema @ship.json "Safe to ship? Which bump?" > events.jsonl
echo $?                                      # 0 done, 1 no data, 2 exhausted, 3 error
jq -c 'select(.type == "run.completed") | {status, data}' events.jsonl
# {"status":"done","data":{"bump":"patch","ship":true}}
```

## Install

```bash
curl -fsSL https://llx.sh/install | sh
```

On macOS (Apple silicon and Intel) and Linux (x86_64 and arm64). It puts `llx`, and `lx`, a link to it, in `~/.local/bin` (or `LLX_INSTALL_DIR`), says so when that directory is not on your `PATH`, and checks the archive against the release's `SHA256SUMS`. `llx.sh/install` is [scripts/install.sh](scripts/install.sh) as `main` has it (the site serves it; `https://raw.githubusercontent.com/rsaryev/llx/main/scripts/install.sh` is the same file). To pin a release:

```bash
curl -fsSL https://llx.sh/install | LLX_VERSION=0.2.0 sh
```

Or take the same archive with [mise](https://mise.jdx.dev) (`mise use -g github:rsaryev/llx`) or [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) (`cargo binstall --git https://github.com/rsaryev/llx llx`), or build it with Rust 1.88 or later: `cargo install --locked --git https://github.com/rsaryev/llx llx`. In a CI job that runs in an image, `ghcr.io/rsaryev/llx:<version>` has llx with `git`, `jq` and `curl`.

Then run `llx` in a project and type `/login`: a ChatGPT sign-in in the browser, an OpenRouter key, or a key for any other provider of the catalog. A provider's key in its variable (`OPENROUTER_API_KEY`) works too, and is what CI uses. `llx doctor` checks the sign-ins, the models and the shell.

## In CI and scripts

In CI llx is the same agent as at your terminal, given one task. It works through the plain filesystem and primitive commands: `bash` with `gh`, `git`, `jq` and your project's own tools. There is no MCP server, no GitHub tool and no built-in prompt. Your workflow owns the prompt and the posting, so the same task runs on GitHub, GitLab or any runner with a shell. The job that runs the model holds a read-only token, and a second job posts what it answered: the write token is never near the model.

```yaml
# a step of a job, after actions/checkout
- uses: rsaryev/llx@v0
  id: llx
  env:
    OPENROUTER_API_KEY: ${{ secrets.OPENROUTER_API_KEY }}
    GH_TOKEN: ${{ github.token }} # read-only in this job
    PR: ${{ github.event.pull_request.number }}
  with:
    model: anthropic/claude-haiku-5.5
    allow: bash
    schema: '{"type":"object","properties":{"summary":{"type":"string"}},"required":["summary"]}'
    prompt: |
      Review pull request #$PR with gh and git. Run something to confirm a problem
      before you report it.
```

The answer is `steps.llx.outputs.result`, JSON matching your schema; the status, the spend and the exit code are outputs too. The action spells the status its own way (`no-data`, `out-of-budget`, `not-ready`): [its outputs](docs/ci.md#outputs).

How a task ends is a contract a pipeline can read. The exit code says it, and `--json` ends with a `run.completed` event carrying the same `status`:

| Exit code | `status` | Meaning |
| --- | --- | --- |
| 0 | `done` | The model answered (with `--schema`, its data matched) |
| 1 | `no_data` | Only with `--schema`: the data did not match it, after two tries to fix it |
| 2 | `exhausted` | Turns (`--max-turns`), time (`--timeout`) or money (`--budget`) ran out before the model answered: the first reached ends the task |
| 3 | `error` | No key, no network, a model error, a bad argument |
| 130 | `interrupted` | Ctrl-C, SIGTERM or SIGHUP |

For a choice among a few options, `llx decide` asks a small decision model about stdin, in about a second for a fraction of a cent, and prints the option it picked. It needs an OpenRouter key. With `--expect`, it exits 1 when the model picks another option, so a script can gate on it:

```console
$ git diff origin/main... | llx decide "Does this change break the public API?" --expect no
```

To search the code by what a question means, `llx grep` ranks the files by the question's words, then has the decision model read the best of them and keep those that answer; each comes back with its lines, whole functions where they are short. On 24 questions about this repository with known answers, its first five files held 98% of the files each answer must name, against 46% for the `rg` pattern an agent would type first, in 2 to 3 s for $0.005 a question ([benchmark/grep](benchmark/grep/README.md)). Without a key it ranks by the words alone:

```console
$ llx grep "where is a failed HTTP request sent again"
```

Ready-made GitHub workflows, each run for real: [pull request review](examples/github/review.yml) with inline comments, [`@llx` in any comment, issue or review thread](examples/github/llx.yml), [issue triage](examples/github/triage.yml), [a failed run explained](examples/github/ci-failure.yml), [a described pull request](examples/github/describe.yml), [a draft release](examples/github/release.yml), [a daily gate on a rule you state](examples/github/gate.yml) and [a weekly audit](examples/github/audit.yml) and [a Jira work item made a draft pull request](examples/github/jira.yml); and [breaking changes to an HTTP API](examples/breaking-changes/README.md), a script for any CI: [examples/](examples/README.md). The action, the two-job pattern, rules, keys, limits, security and GitLab: [docs/ci.md](docs/ci.md). Shell recipes with `gh` and `glab`: [docs/recipes.md](docs/recipes.md).

## In other agents

Claude Code, Codex, another agent or a script of yours can call llx from its shell, as its task needs. `llx exec` hands a task to a helper on any provider and model, so the agent's own tokens go where they matter; `llx grep` finds the code that answers a question, with its lines; `llx decide` picks one of a few options about some text, or about each of many files, in about a second.

In Claude Code, one command installs both as a plugin ([examples/claude-code](examples/claude-code/README.md)):

```text
/plugin install llx --marketplace rsaryev/llx
```

Its `llx` subagent hands bulk reading (many files, a long log, a first pass over a diff) to llx on the model you chose with `/login`, a ChatGPT plan you already pay for among them; a small Claude model only relays it. As each session starts, the plugin tells Claude what of llx this machine can run, and where an OpenRouter key is set (`llx decide --check`), `llx grep` as its first search where it does not know the exact name, and one rule for `llx decide`: when a judgment has a few options and the text is long, comes in many items or is untrusted, ask it rather than read it all. Asked to sort 20 issue texts into bugs and features, Claude ran one command, `llx decide "Is this issue a bug report or a feature request?" --choices "bug=…,feature=…" --json issues/*`, and read none of the files: $0.0005 of decisions.

For Codex, or any agent that reads `AGENTS.md`, the same is a few lines to paste ([examples/codex](examples/codex/README.md)). Everything else: [docs/agents.md](docs/agents.md).

## Documentation

[Using llx](docs/usage.md) · [CI](docs/ci.md) · [Recipes](docs/recipes.md) · [Tools](docs/tools.md) · [Modes and trust](docs/modes.md) · [In other agents](docs/agents.md) · [Reference](docs/reference.md) · [Daemon](docs/daemon.md) · [Architecture](ARCHITECTURE.md) · [Models](MODELS.md) · [Changelog](CHANGELOG.md)

## Contributing

Start with [CONTRIBUTING.md](CONTRIBUTING.md) and [ARCHITECTURE.md](ARCHITECTURE.md). Report vulnerabilities as [SECURITY.md](SECURITY.md) describes. llx is open source under the [MIT License](LICENSE), made by [Localix](https://localix.ai).
