<p align="center">
  <img src="site/public/logo.svg" width="72" height="72" alt="Localix CLI">
</p>

<h1 align="center">Localix CLI</h1>

<p align="center">
  <strong>An ultrafast, lightweight agent runtime for coding and CI</strong>
</p>

<p align="center">
  <code>llx exec</code> runs one task and ends with an answer and an exit code your pipeline can branch on. One binary under 4 MB, on 1,000+ models from 200+ providers: a ChatGPT sign-in, OpenRouter or your own key. A minimal agent at your terminal, too.
</p>

<p align="center">
  <a href="https://github.com/localixai/localix-cli/actions/workflows/ci.yml"><img src="https://github.com/localixai/localix-cli/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI status"></a>
  <a href="https://github.com/localixai/localix-cli/releases/latest"><img src="https://img.shields.io/github/v/release/localixai/localix-cli?color=black&label=release" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-black" alt="MIT License"></a>
  <img src="https://img.shields.io/badge/macOS%20%C2%B7%20Linux-black" alt="macOS and Linux">
  <img src="https://img.shields.io/badge/binary-under%204%20MB-black" alt="One binary under 4 MB">
  <img src="https://img.shields.io/badge/Rust-black?logo=rust" alt="Written in Rust">
  <img src="https://img.shields.io/badge/built%20and%20reviewed%20with-Claude%20Fable%205.2-black" alt="Built and reviewed with Claude Fable 5.2">
</p>

<p align="center">
  <a href="#try-it">Try it</a> ·
  <a href="#how-it-works">How it works</a> ·
  <a href="#at-your-terminal">At your terminal</a> ·
  <a href="#in-ci-and-scripts">In CI</a> ·
  <a href="#in-other-agents">In other agents</a> ·
  <a href="#measured-beside-claude-code">Measured</a> ·
  <a href="#documentation">Documentation</a> ·
  <a href="https://llx.sh">llx.sh</a>
</p>

<p align="center">
  <a href="https://llx.sh">
    <img src="docs/assets/demo.gif" alt="llx at a terminal, as on llx.sh, every step a real run on Claude Haiku 5.5: a model is picked, a failing test is fixed, two questions are answered with steps, tabs, a table and a formula, two agents work in parallel, then llx decide and llx exec --schema answer from the shell" width="880">
  </a>
</p>

<details>
<summary><b>What the recording shows</b>: 84 seconds, every step a real run on Claude Haiku 5.5</summary>

1. **Any model.** `/model` lists the 367 models of every provider signed in, and one is picked.
2. **A fix.** The failing median test in `stats.py` is fixed without a question: 7 seconds, $0.0012.
3. **Answers drawn as what they are.** One with numbered steps and a tab for each of pip, uv and Poetry; one with a table and the formula of the mean, written as a terminal can.
4. **Agents in parallel.** One adds `mode()` with a test, one writes the README, and their work is checked.
5. **A decision.** From the shell, as CI calls it, `llx decide` answers a yes-or-no question about the diff in 475 ms.
6. **Data.** `llx exec --schema` reads the diff and answers in the shape a JSON Schema defines.

The same recording plays on [llx.sh](https://llx.sh), where it can be paused, scrolled and copied from.

</details>

llx is a model in a loop over five tools, on any provider. One binary, three uses:

| Who calls | Command | What they get |
| :-- | :-- | :-- |
| **Your CI and scripts** | `llx exec --json --schema …` | One task, then exit: the answer in the shape you define, JSON events, an exit code to branch on |
| **You** | `llx` | A minimal agent at your terminal |
| **Your other agents** | `llx exec` · `llx decide` | Commands they run from their shell: a helper on any model, a quick decision. They extend the agent you use, never replace it |

## Try it

```bash
curl -fsSL https://llx.sh/install | sh
cd your-project && llx    # a session: /login first, then a task
```

And the same agent as a step of a script, answering with data:

```bash
echo '{"type":"object","properties":{"bump":{"enum":["patch","minor","major"]},"ship":{"type":"boolean"}},"required":["bump","ship"]}' > ship.json
git diff main... | llx exec --json --mode plan --schema @ship.json "Safe to ship? Which bump?" > events.jsonl
echo $?    # 0 done, 1 no data, 2 exhausted, 3 error
jq -c 'select(.type == "run.completed") | {status, data}' events.jsonl
# {"status":"done","data":{"bump":"patch","ship":true}}
```

> [!TIP]
> `llx doctor` checks the sign-ins, the models and the shell, and says what is missing.

## How it works

The five tools are `bash`, `apply_patch`, `ask`, `agents` and `jobs` ([tools](docs/tools.md)). A task ends when the model replies with a message and no tool call: that message is the answer. With `--schema`, llx then asks the model once more for data matching your JSON Schema.

- **Sandboxed by default.** llx works in your project without asking: commands run in the system's sandbox, which writes only inside the workspace (never its `.git`) and opens no network, and file changes there are made[^sandbox]. What must reach further is asked first, and refused where no one is there to ask.
- **Modes say the rest.** `plan` runs read-only in the same sandbox, `bypass` (`-y`) allows everything, and rules or a mode of your own cover what is between ([modes](docs/modes.md)).
- **Bounded.** A task is held to `--max-turns`, `--timeout` and `--budget`: the first reached ends it.
- **Any model.** It runs on 200+ model providers: a ChatGPT sign-in, OpenRouter, or a key ([models](MODELS.md)). With an OpenRouter key, a small decision model advises each task at its edges: it picks the reasoning level as a conversation opens (`--reasoning auto`, the default), and takes a second look at an answer that may leave work undone (`--no-advice` turns it off).
- **Saved.** Every run is a session: `llx -c` goes on with the newest one in this directory, `llx sessions` lists them and `llx resume ID` opens one; `/new` opens another beside it ([sessions](docs/usage.md#sessions)).

## At your terminal

What finished goes into the terminal's own scrollback, so scrolling, selecting and copying stay the terminal's. An answer is drawn from its Markdown, with no markup left to read past, and the model is told the forms an answer may take: a file tree, code under the file it names, a diff in green and red, a formula in the Unicode a terminal has, steps down a rail, tabs, figures side by side, a table with columns sized by what they hold.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="packages/markdown/assets/files-dark.svg">
    <img src="packages/markdown/assets/files-light.svg" alt="An answer as llx draws it in a terminal: a file tree, code under its file's name, a diff, a formula, a quote with its source and a footnote" width="49%">
  </picture>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="packages/markdown/assets/report-dark.svg">
    <img src="packages/markdown/assets/report-light.svg" alt="An answer as llx draws it in a terminal: figures side by side, a table, a task list, the case for and against, and sources by number" width="49%">
  </picture>
</p>

<p align="center">
  <sub>The renderer's own rows, written as SVG; they follow your GitHub theme. How they are read and drawn: <a href="packages/markdown/README.md">packages/markdown</a>.</sub>
</p>

| Key | What it does |
| :-- | :-- |
| <kbd>Enter</kbd> | Sends. While a task works, what you type goes to it, read at its next step |
| <kbd>Tab</kbd> | While a task works, keeps what you typed for after it |
| <kbd>Shift</kbd>+<kbd>Tab</kbd> | Goes round the modes: `default` → `plan` → `bypass` → your own |
| <kbd>Esc</kbd>,&nbsp;<kbd>Ctrl</kbd>+<kbd>C</kbd> | Step back out of the innermost thing open; <kbd>Ctrl</kbd>+<kbd>C</kbd> also stops work |
| <kbd>/</kbd> | Lists the commands: `/login`, `/model`, `/mode`, `/new`, `/rewind`, `/diff`, `/copy`, `/usage` and more |
| <kbd>@</kbd> | Lists the workspace's files to mention; a dragged file or picture is attached |
| <kbd>←</kbd> | On an empty prompt, lists the sessions in this project: several can work at once |

Everything a session does, key by key: [Using llx](docs/usage.md#in-a-session).

## Measured beside Claude Code

The same task, the same model, the same Anthropic key, started in the same minute; the tests that score a run were written before it. On the larger task llx finished in 60% of the time for a ninth of the cost; on the small one the two are even.

| Task | Model | Agent | Solved | Time | Turns | Cost |
| :-- | :-- | :-- | --: | --: | --: | --: |
| evalkit, 238&nbsp;tests | Haiku&nbsp;5.5 | **llx** | 238&nbsp;of&nbsp;238 | **286&nbsp;s** | 14 | **$0.050** |
| | | Claude&nbsp;Code&nbsp;2.1.321 | 238&nbsp;of&nbsp;238 | 478&nbsp;s | 25 | $0.46 |
| shopkit, 69&nbsp;tests | Haiku&nbsp;5.5 | **llx** | 69&nbsp;of&nbsp;69 | 69&nbsp;s | 5 | $0.012 |
| | | Claude&nbsp;Code&nbsp;2.1.321 | 69&nbsp;of&nbsp;69 | 67&nbsp;s | 4 | $0.011 |

> [!NOTE]
> One run of each agent, on one machine: a measurement, not a law.

Every run's log, the code it wrote and its test output are in [benchmark/](benchmark/README.md), and [the history](benchmark/HISTORY.md) has every run made, the slower ones too. `bun benchmark/run.ts` repeats it on your key.

## Install

```bash
curl -fsSL https://llx.sh/install | sh
```

On macOS (Apple silicon and Intel) and Linux (x86_64 and arm64). It puts `llx`, and `lx`, a link to it, in `~/.local/bin` (or `LLX_INSTALL_DIR`), says so when that directory is not on your `PATH`, and checks the archive against the release's `SHA256SUMS`. `llx.sh/install` is [scripts/install.sh](scripts/install.sh) as `main` has it (the site serves it; `https://raw.githubusercontent.com/localixai/localix-cli/main/scripts/install.sh` is the same file). To pin a release:

```bash
curl -fsSL https://llx.sh/install | LLX_VERSION=0.3.0 sh
```

| Another way | Command |
| :-- | :-- |
| [mise](https://mise.jdx.dev), the same archive | `mise use -g github:localixai/localix-cli` |
| [cargo-binstall](https://github.com/cargo-bins/cargo-binstall), the same archive | `cargo binstall --git https://github.com/localixai/localix-cli localix-cli` |
| Built with Rust 1.89 or later | `cargo install --locked --git https://github.com/localixai/localix-cli localix-cli` |
| An image, for a CI job that runs in one | `ghcr.io/localixai/localix-cli:<version>`, llx with `git`, `jq` and `curl` |

Then run `llx` in a project and type `/login`: a ChatGPT sign-in in the browser, an OpenRouter key, or a key for any other provider of the catalog. A provider's key in its variable (`OPENROUTER_API_KEY`) works too, and is what CI uses. `llx doctor` checks the sign-ins, the models and the shell.

<details>
<summary><b>A model on your own machine, or any other host</b></summary>

A model on your own machine needs no sign-in: llx finds Ollama, LM Studio and llama.cpp's `llama-server` where each listens, and `/model` lists the models they run under `Local:` (`llx exec --provider ollama -m qwen3:8b …` in a script).

Any other host that speaks OpenAI's chat completions, on your network or anywhere, goes in `~/.config/llx/settings.json` and is listed under `Hosts:` (`"local"` instead of `"hosts"` for another server on your machine):

```json
{"hosts": [{"name": "work", "url": "https://llm.example/v1", "key": "…"}]}
```

`/login` adds one without the file: its last row, `Another host`, asks the address, a name and the key (Enter alone for none), checks that the host answers, saves it (under `local` for an address on this machine, else `hosts`) and goes on to its models.

</details>

## In CI and scripts

In CI llx is the same agent as at your terminal, given one task. It works through the plain filesystem and primitive commands: `bash` with `gh`, `git`, `jq` and your project's own tools. There is no MCP server, no GitHub tool and no built-in prompt. Your workflow owns the prompt and the posting, so the same task runs on GitHub, GitLab or any runner with a shell. The job that runs the model holds a read-only token, and a second job posts what it answered: the write token is never near the model.

```yaml
# a step of a job, after actions/checkout
- uses: localixai/localix-cli@v0
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
| --: | :-- | :-- |
| 0 | `done` | The model answered (with `--schema`, its data matched) |
| 1 | `no_data` | Only with `--schema`: the data did not match it, after two tries to fix it |
| 2 | `exhausted` | Turns (`--max-turns`), time (`--timeout`) or money (`--budget`) ran out before the model answered: the first reached ends the task |
| 3 | `error` | No key, no network, a model error, a bad argument |
| 130 | `interrupted` | Ctrl-C, SIGTERM or SIGHUP |

For a choice among a few options, `llx decide` asks a small decision model about stdin, in about a second for a fraction of a cent, and prints the option it picked. It needs an OpenRouter key. With `--expect`, it exits 1 when the model picks another option, so a script can gate on it:

```console
$ git diff origin/main... | llx decide "Does this change break the public API?" --expect no
```

With `rg`, it makes a search of the code by what it does: `rg -l` narrows the files by a few likely words, and `llx decide` reads each and keeps those that do the thing, under whatever name:

```console
$ rg -l -i 'retry|backoff|again' src | xargs llx decide "Does this file send a failed request again?"
```

Ready-made GitHub workflows, each run for real:

| Workflow | What it does |
| :-- | :-- |
| [review.yml](examples/github/review.yml) | Pull request review, with inline comments |
| [llx.yml](examples/github/llx.yml) | `@llx` in any comment, issue or review thread |
| [triage.yml](examples/github/triage.yml) | Issue triage |
| [ci-failure.yml](examples/github/ci-failure.yml) | A failed run explained |
| [describe.yml](examples/github/describe.yml) | A described pull request |
| [release.yml](examples/github/release.yml) | A draft release |
| [gate.yml](examples/github/gate.yml) | A daily gate on a rule you state |
| [audit.yml](examples/github/audit.yml) | A weekly audit |
| [deps.yml](examples/github/deps.yml) | A dependency update read |
| [jira.yml](examples/github/jira.yml) | A Jira work item made a draft pull request |

And two folders of scripts for any CI: [breaking changes to an HTTP API](examples/breaking-changes/README.md), and [secrets kept out of an agent's context, a commit and a pull request](examples/secrets/README.md). How to install a workflow: [examples/](examples/README.md). The action, the two-job pattern, rules, keys, limits, security and GitLab: [docs/ci.md](docs/ci.md). Shell recipes with `gh` and `glab`: [docs/recipes.md](docs/recipes.md).

## In other agents

Claude Code, Codex, another agent or a script of yours can call llx from its shell, as its task needs. `llx exec` hands a task to a helper on any provider and model, so the agent's own tokens go where they matter; `llx decide` picks one of a few options about some text, or about each of many files, in about a second.

A skill tells any agent that takes skills what llx does and when each part pays: the coding agent, one task in a script or CI and the GitHub Action, helpers on another model, and `llx decide` (Claude Code, Codex, Cursor, Gemini CLI, OpenCode and others):

```bash
npx skills add localixai/localix-cli
```

In Claude Code it is also a plugin: `/plugin install llx --marketplace localixai/localix-cli`. The skill is a thin stub that points the agent at each command's `--help`, so what it reads always matches the installed llx. Everything else: [docs/agents.md](docs/agents.md).

## Documentation

| Guide | What it holds |
| :-- | :-- |
| [Using llx](docs/usage.md) | The command line, a session key by key, signing in and choosing a model, sessions, project instructions |
| [CI](docs/ci.md) | The GitHub Action, the two-job pattern, rules, keys, limits, security, GitLab |
| [Recipes](docs/recipes.md) | Shell recipes with `gh` and `glab` |
| [Tools](docs/tools.md) | The five tools: `bash`, `apply_patch`, `ask`, `agents`, `jobs` |
| [Modes and trust](docs/modes.md) | Modes, rules, approvals, the sandbox and the trust boundaries |
| [In other agents](docs/agents.md) | Helpers on any model, `llx decide`, the skill |
| [Reference](docs/reference.md) | How a task ends, the exit codes, the `--json` events, `llx decide` |
| [Compared](docs/compare.md) | The action beside other agents' actions |
| [Architecture](ARCHITECTURE.md) · [Models](MODELS.md) · [Changelog](CHANGELOG.md) | The layers and their rules, the model catalog, what changed |

## Contributing

Start with [CONTRIBUTING.md](CONTRIBUTING.md) and [ARCHITECTURE.md](ARCHITECTURE.md). Report vulnerabilities as [SECURITY.md](SECURITY.md) describes. llx is open source under the [MIT License](LICENSE), made by [Localix](https://localix.ai).

[^sandbox]: Seatbelt on macOS; Landlock with a seccomp filter on Linux (6.2 or later), where commands only read. The boundaries are in [Modes and trust](docs/modes.md#trust-boundaries).
