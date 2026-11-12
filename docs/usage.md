# Using llx

The everyday reference: the command line, a session's keys and slash commands, sign-in and
models, and how sessions are kept. For the short version see the [README](../README.md).

- [Command line](#command-line)
- [In a session](#in-a-session)
- [Signing in and choosing a model](#signing-in-and-choosing-a-model)
- [Sessions](#sessions)
- [Project instructions](#project-instructions)

## Command line

`llx` opens a session at the terminal, and `llx exec` (or `-p`) runs one task and exits. `llx decide` asks one question about stdin and prints the option the decision model picked.

```bash
llx                                                        # a session at the terminal
llx "fix the failing test in src/parse.rs"                 # a session that starts with this task
llx -c                                                     # go on with the newest session here
llx exec "fix the failing test in src/parse.rs"            # one task, then exit (also: llx e, llx -p)
llx exec -y "run the tests and fix what fails"             # allow commands and edits without asking
llx --mode plan "how should we split src/parse.rs?"        # read-only: change no file
git diff | llx -p "review this change"                     # piped stdin is added to the prompt
llx exec - < task.md                                       # the prompt from stdin
llx exec -y "fix the lint" </dev/null                      # from a script whose stdin stays open
llx exec -C ../site --allow 'bash(curl *)' "check that sign-up works on http://localhost:3000"
llx exec --json -y "update the changelog" | jq -c 'select(.type == "tool.call")'
llx exec --schema '{"type":"object","properties":{"version":{"type":"string"}},"required":["version"]}' \
    "which version is this crate?"                        # the data alone on stdout
llx exec resume --last now update the README               # one more task in the newest session
git diff | llx decide "Does this break the public API?"    # yes or no, in about a second
```

With `exec`, given a PROMPT, stdin that is not a terminal is read (up to 256 KiB) and added to the
prompt in a `<stdin>` block; when it is a pipe, llx says so on stderr first. A caller whose stdin
stays open (a harness with a pipe, `ssh` without `-n`, a `while read` loop) should pass
`</dev/null`.

The `curl` example needs its rule: in `default` a command opens no network, so without one the
model must escalate it, which an `exec` with no terminal refuses (there is no one to ask).

### Options

| Option | Meaning |
| --- | --- |
| `-C, --cd DIR` | The workspace: paths resolve in it and files change only inside it. Default: the current directory. |
| `-i, --image FILE` | Pictures (PNG, JPEG, GIF, WebP) attached to the first task, in a session or in `exec`. Repeat it, or separate them with commas. A file that is missing or not a picture is a usage error, before any model is called. |
| `--mode MODE` | A named set of rules: `default`, `plan`, `bypass`, or your own from `settings.json` (see [Modes and approvals](modes.md)). Also `--permission-mode`. Default: `default`. |
| `--allow RULE`, `--deny RULE` | Add a rule to the mode's for this run: `bash(cargo test *)`, `bash(git push *)`, `jobs` ([the syntax](modes.md#rules)). Repeat them. A deny refuses in every mode, `-y` included. |
| `-y, --yes` | `--mode bypass`: allow commands and file changes without asking. Also `--yolo` and `--dangerously-skip-permissions`. Giving it with `--mode` is a usage error. |
| `-p, --print` | Run the prompt as one task and exit, as `exec` does. |
| `-c, --continue` | Go on with the newest session of this directory, as `resume --last` does. |
| `--json` | With `exec` or `-p`: one JSON event per line on stdout (see [`--json`](reference.md#json-events)). |
| `--schema SCHEMA` | With `exec` or `-p`: end with data matching this JSON Schema (inline, or `@file`), printed alone on stdout. |
| `-o, --output-last-message FILE` | With `exec` or `-p`: also write the final answer, or the data with `--schema`, to FILE. With `--schema` and no data that matched, the file is left empty. |
| `-v, --verbose` | Show each tool result and model turn in the trace. With `--json`, the trace also goes to stderr. |
| `--provider PROVIDER` | Where the agent's chat model comes from: a provider of the model catalog by id: `openai` (a ChatGPT sign-in or a key), `openrouter`, `anthropic`, `deepseek`, `xai`… Env `LLX_PROVIDER`. Default: the one `/login` saved, else the one signed in to. |
| `-m, --model MODEL` | The agent's chat model, a model id of the provider's (any OpenRouter id works). Env `LLX_MODEL`. Default: the one `/login` saved, else `gpt-6-luna` on OpenAI and `anthropic/claude-haiku-5.5` on OpenRouter. |
| `--reasoning LEVEL` | How much the chat model reasons: `auto`, `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max` or `ultra`, as far as the model takes it (a level it does not take is a usage error; `/login` lists its levels). Env `LLX_REASONING`. Default: the one `/login` saved for that model, else `auto`: the decision model picks a level from `low` to `xhigh` as each conversation opens, and the conversation keeps it (the model's own default with no decision model). |
| `--no-advice` | No advice from the decision model: no level picked under `auto`, no second look at whether a task is done. Env `LLX_NO_ADVICE`. The advice needs an OpenRouter key; without one nothing changes. |
| `--decision-model MODEL` | `llx doctor` only: the decision model it checks (a session or `exec` refuses the flag). Env `LLX_DECISION_MODEL`, default `~typesafe/jev-latest`. |
| `--max-turns N` | Model turns for one task (1 to 10000). None by default: `--timeout` bounds a task in `exec`. |
| `--budget USD` | Dollars one task may spend (more than 0, e.g. `0.50`). It stops at the next model turn once its turns, its agents' and the decision model's advice have cost that. A cost is the provider's where it reports one (OpenRouter), else the catalog's price for the tokens (a ChatGPT plan's requests at their API-equivalent value); a model the catalog has no price for never reaches it. |
| `--timeout SECONDS` | Seconds one task may take (at most 86400; by default 1800 in `llx exec`, and none in a session, where you stop a task yourself). At the limit a command or agent running is killed, and the task stops at once, as out of time (exit 2), or while it waits on its monitors or agents. Time spent waiting on you (an approval, a question) does not count. |

### Commands

- `exec [PROMPT | -]` (alias `e`): run one task without a session and exit, with the answer alone on stdout. `exec resume [ID | --last] [PROMPT]` runs one more task in a saved session. In CI, the [GitHub Action](ci.md#the-github-action) runs it.
- `decide QUESTION [FILE…]`: ask the decision model one question about stdin, or about each file on its own (`--choices a,b,…` instead of yes or no, `--expect CHOICE` to exit 1 on any other, `--json` for the probabilities and the cost), and print the option it picked. No agent, no tools; it needs an OpenRouter key. The [reference](reference.md#llx-decide) has its options and exit codes.
- `resume [ID | --last] [PROMPT]`: go on with a session at the terminal. `--last` is the newest session that ran in this workspace (else the newest of all, with a note).
- `sessions`: list the saved sessions, newest first, with the directory each ran in.
- `logout`: forget every saved sign-in: the ChatGPT tokens and every provider's saved key. A key still set in the environment is named, to unset. (Signing in is `/login` in a session.)
- `doctor`: check the sign-ins, both models (the agent's and the decision model, `--decision-model`) and the shell.

## In a session

A session needs a terminal on stdin that its interface can draw on; without one (or where the
interface cannot start) it is a usage error pointing to `llx exec`, which prints the same blocks
(each call, the answer, how it ended) without the live part.

### The screen

- What finished goes into the terminal's own scrollback, so scrolling, selecting and copying are the terminal's. Only the bottom is redrawn: the call running, `Working (12s · esc stop)`, the messages you queued meanwhile, and the prompt `❯`.
- Under the prompt, a list shows the session, the agents it started (as a branch) and its jobs; with no agents and no jobs it is gone. Each row ends its time and tokens, or how a job runs or ended, at the right edge, as the footer ends its spend. The other sessions take one line, `← 2 other sessions (1 working, 1 waiting for you)`.
- ↓ from an empty prompt gives the list the keys: ↑↓ choose, Enter or → shows an agent's transcript, what a command in the background prints (as it prints) or what a monitor's last check printed, `x` stops an agent or a job, Esc or ← go back. A command or a monitor that ended stays listed with how (`✓`, or red `✗` when it failed or was stopped) until `x` removes it. One you stop opens no task: the model reads it with your next.
- Each session gets a short title from its first task, made in the background by the same model at its lowest reasoning level (the task never waits for it), shown in the list and saved with the session.
- A `bash` call shows its command and the end of its output.
- An answer is drawn from its Markdown, with no markup left to read past: headings by their weight, lists (a bullet that says how deep the list is; a task's mark, `✓` or `☐`, where its bullet would stand), code with its tabs as columns under the file its fence names, a `diff` fence in green and red, a ` ```tree ` fence as a file tree, a formula (`$x^2$`, `$$ … $$`) as the Unicode a terminal has (`x²`), footnotes by number, a quote's source under it, a link's address after its text. A table's columns are aligned and given room by what they hold, its cells wrap, and its rows are ruled apart when they hold long text; on very narrow terminals, cells are shown vertically. A command in backticks stays on one row where a row holds it.
- The model is told the forms an answer may take, and they are drawn as what they are: `> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]` and `[!CAUTION]` a callout behind a bar of its colour, `> [!STEPS]` stages down a rail, each name on a row of its own, `> [!TABS]` each variant under its label, `> [!STATS]` figures side by side, each number over what it counts (a list where no row holds them), `> [!PROS]` and `> [!CONS]` by `+` and `-`, `> [!DETAILS]` a section under its title, `> [!SOURCES]` links by number, each address on a row of its own. Where the answer is not drawn (a pipe, `--json`), the model is not told them and the answer is its text as written.
- Consecutive successful calls of the same verb (`Ran`, `Patched`) are joined under one bullet with `↳`; a failure, a question or a note between them starts a new group.

### Typing

- Enter sends. Shift+Enter, Alt+Enter, Ctrl-J or a line ending in `\` start a new line; a paste keeps its lines. Up and Down walk earlier entries.
- While a task works, Enter sends what you type to it, read at its next step (`↳`); Tab keeps it for after the task (`⏎`).
- Ctrl-R finds the last earlier entry with the words already typed (type them first; again for older ones). Ctrl-G writes the entry in `$EDITOR`. Ctrl-A and Ctrl-E go to the line's start and end; Ctrl-U clears the line before the cursor and Ctrl-K after it; Ctrl-W (or Alt+Backspace) deletes the word before it; Alt or Ctrl with ← → moves by word. Shift+Left and Shift+Right select (with Alt or Ctrl, by word; Shift+Home and Shift+End to the line's ends): Backspace or Delete deletes the selection, and typing or a paste replaces it. Ctrl-Z gives the terminal back and stops llx, as vim does; the shell's `fg` continues it with the screen drawn again, and the session's tasks go on meanwhile.
- Leaving (Ctrl-C twice, Ctrl-D, `/exit`) prints, dim under the history, for each session with a task (the one shown first) what it spent, `Session usage: 24.1k in · 87% cached · 1.2k out · $0.042`, and `Resume this session: llx resume <id>`.
- Clipboard text shows as `[Paste]` (numbered for several) and is expanded when sent.
- Shift+Tab goes round the modes (default → plan → bypass → your own → default) without Enter; while a task runs, the switch waits until it ends.

### Stepping back: Esc and Ctrl-C

Esc and Ctrl-C each undo the innermost thing open, one step at a time; Ctrl-C also stops work. In order: a line typed in a question goes back to its options; a search's query is cleared (Ctrl-U clears it too); a list closes; an approval or a question closes (Esc answers no, Ctrl-C answers no and stops the task that asked); a step of `/login` goes back to the one before; in an agent's view Ctrl-C stops that agent alone, then goes back to the main view; a task running stops (while it stops, a second Ctrl-C quits at once, as the footer says); a draft is cleared (↑ brings it back); and on an empty prompt the first Ctrl-C says `ctrl-c again to quit`, the second quits (asked first while work runs). Ctrl-D on an empty prompt quits too, asked first the same way. [Stopping and quitting](modes.md#stopping-and-quitting) says what a stop does and what quitting asks.

Each list's last line names only the keys that do something there, each with what it does: `tab provider · ←→ reasoning · enter select · esc cancel`.

### Attaching files

- Drag a file into the terminal or paste its path: a picture (PNG, JPEG, GIF, WebP, up to 8 MB) becomes `[image: shot.png]`, sent to the model as one from a copy kept in `~/.llx/pasted`; anything else becomes `[file: notes.md]`, sent as its path for the agent to read. Several at once work.
- `@` followed by a few letters lists the workspace's files; Enter or Tab puts the chosen path in.
- Ctrl+V attaches the clipboard's picture (macOS).
- `-i FILE` (`--image`) attaches pictures to the first task, in a session or in `exec`; a file that is missing or not a picture is a usage error before any model is called.

### Slash commands

Typing `/` lists them. Tab completes the highlighted command (`/mode ` with a space, then a mode's name); Enter runs the typed command when it is a whole one (`/mode`, though `/model` is highlighted), else the highlighted one (`/mo` runs `/model`). A session's first message is read the same way: `llx "/status"` carries out `/status`.

| Command | What it does |
| --- | --- |
| `/login` | Signs in, and chooses the provider, the model and its reasoning level (see [below](#signing-in-and-choosing-a-model)). |
| `/model` (also `/models`) | Chooses the model and its reasoning level. |
| `/mode [name]` (also `/permissions`) | Lists the [modes](modes.md) or switches to one. |
| `/new` (also `/clear`) | Opens a new session and shows it, even while a task works; the old one goes on in the background. |
| `/status` | The model and how it is signed in, the workspace, the mode, the session. |
| `/rewind` (also `/undo` or `/checkpoint`, or Esc Esc on an empty prompt) | Goes back in the conversation to before one of its tasks (one picker, "Rewind the conversation to before"): the task's words come back in the prompt and the session's agents are let go. The files stay as they are: nothing records what a command changed. |
| `/diff` | The changes since the last commit, new files too. |
| `/init` | Writes an `AGENTS.md` for the project. |
| `/copy` | Copies the last answer that says something. |
| `/usage` (also `/cost`) | What the session spent. The footer's right end says it all along (`24.1k in · 87% cached · 1.2k out · $0.042`), a resumed session's earlier tasks included. |
| `/help` (also `/?`), `/exit` (also `/quit`, or Ctrl-D) | Help; leave. |

### Several sessions

- Each session is recorded in its own file, and Ctrl-C stops only the one shown.
- ← on an empty prompt lists the sessions in this project: those open (working, then waiting for you, then idle or done), "+ New session", then this directory's saved sessions, newest first. ↑↓ choose, Enter or → opens one (a saved one is resumed in a session of its own), `x` stops a session's task without switching to it, Esc or ← close the list.
- At most 8 are open: another closes the oldest idle one, and with none idle nothing opens (`8 sessions are working: stop one first`).
- An approval or a question from a session not shown names it (`session <name> (in the background)`), ignores keys for 300 ms and is answered as any other; its own choices wait until it is shown.
- When a session not shown finishes, the one shown says `● Session "…" finished · ← to open`.

## Signing in and choosing a model

`/login` goes step by step:

1. A provider: every one of the catalog in one list, the popular first, then the current one and those signed in to, then the rest by name; `✓` marks those signed in to or with a key set; type to search by name or id.
2. For one signed in to already: `Keep it` or `Sign in again`.
3. For one with two ways to sign in (OpenAI, OpenRouter): `Sign in in the browser` or `Paste an API key`. Then the sign-in itself.
4. Its model list: ↑↓ choose the model, ←/→ change its reasoning level, Enter confirms.

Each step is titled by its path (`Login › OpenAI › Model`), and Esc goes back one step, as you left it; at the providers it ends /login. Once a sign-in is saved, Esc at the model goes back to the providers, and ending there says you are signed in and which model is still in use.

- **OpenAI**, with a ChatGPT sign-in: your browser opens OpenAI's sign-in page and comes back to llx on `127.0.0.1:1455` (or 1457), and llx goes on by itself, with nothing to paste; requests are billed to your ChatGPT plan, and `/login` and `/model` offer the models the ChatGPT backend serves, those it serves alone too. On a machine without a browser, open the address shown elsewhere and paste the address the browser ends on.
- **OpenRouter**, with a key: paste it (it is not shown), or sign in in the browser for a new one. `OPENROUTER_API_KEY` works too, and wins over a saved key.
- **Any provider of the model catalog** (Anthropic, Google, xAI, DeepSeek, Mistral, Groq, Moonshot, Z.ai, Together, Fireworks… 200+), with a key: paste it, or set the variable models.dev names for it (`ANTHROPIC_API_KEY`, `DEEPSEEK_API_KEY`…), which wins over a saved key. OpenAI takes an API key too, and is called over its Responses API with it as with a ChatGPT sign-in. Each other is called over OpenAI's chat completions at the address the catalog gives, but a model served over Anthropic's Messages API too (Anthropic's own) is called over it: its prompt cache, its levels and its thinking are only there, and over chat completions each turn would pay for the whole conversation again.

The models, their context windows, reasoning levels and defaults come from one catalog, models.dev's ([MODELS.md](../MODELS.md)): its files for the popular providers are built in, and a session fetches its newest data in the background, at most once an hour (`LLX_MODELS_REFRESH=0` turns that off); what it fetches is listed a minute later. Each request's cost comes from the catalog's prices when the provider reports none (a ChatGPT plan's as its API-equivalent value).

Models on your own machine are not in the catalog: llx asks the servers that run them. It looks for Ollama (`OLLAMA_HOST`, else `127.0.0.1:11434`), LM Studio (`127.0.0.1:1234`) and llama.cpp's `llama-server` (`127.0.0.1:8080`), and for the servers you name in `settings.json`: `"local": [{"name": "box", "url": "http://box.lan:8080"}]` (one named as a known one replaces it). Those that answer are `/model`'s `Local:` tabs, each with the models it runs now; when none answers the line reads `Local: none running`. `--provider ollama`, `lmstudio`, `llama.cpp` or your server's name picks one for a run, with `-m` a model it runs (its first when left out). The context is the one the model is loaded with, as the server says it; a reply is 8,192 tokens at most, a quarter of a smaller context; the reply is streamed, and no title is asked of the model, as such a server answers one request at a time. Installing a model is the server's own to do (`ollama pull`, `lms get`, `llama-server -hf`): it is listed the next time `/model` opens. `LLX_LOCAL=0` looks only for what the settings name.

Any other host that speaks OpenAI's chat completions, wherever it is, is named under `hosts`: `"hosts": [{"name": "work", "url": "https://llm.example/v1", "key": "…"}]`. It is a `Hosts:` tab of `/model` and `--provider work`, called as the catalog's plain hosts are, with none of a local server's limits. Its models are those its `/v1/models` lists; what it does not say goes beside its address: `"models": ["big", "small"]` for a host that lists none (it is then not asked), `"context": 200000` for its models' window, `"output": 16000` for the most a reply writes (both work under `local` too). `key` is left out when the host asks for none; it is kept in `settings.json` as written. A model the catalog knows under some provider (by its id, as a router names it, `anthropic/…`, or bare) takes that entry's reasoning levels, limits and cache marks, and a request the host says no cost for is priced by it: its vendor's price, which the host's own may differ from. Any other model is offered no reasoning level and its requests are not priced. `/login` adds one without the file: its last row, `Another host`, asks the address, a name and the key (Enter alone for none), checks that the host answers, saves it (under `local` for an address on this machine, else `hosts`) and goes on to its models.

`/model` chooses among the models of every provider signed in to, each provider a tab over the list (`OpenAI 36 · [OpenRouter 288]`, with how many models each lists): Tab and Shift+Tab switch, and the list opens on the current model's. ←/→ change the highlighted model's reasoning level inline, and Enter confirms both. The first level is `auto` (the decision model's pick as each conversation opens); the current model keeps its level, and other models start at `auto`. A list longer than 12 is searched by typing (Backspace edits the query, Esc clears it, then cancels; `/model`'s search finds a model by its name or id in every tab, each tab counting its matches, and goes to the tab of the best one when the tab shown has none) and scrolls with ↑↓, PageUp, PageDown, Home and End. Shift+Up and Shift+Down move the level without a menu.

The choice is put in at once, in the same conversation, and saved as the default for later sessions and for `exec`: three keys of `~/.config/llx/settings.json` (under `XDG_CONFIG_HOME` when set), beside your [modes](modes.md#modes), which it keeps:

```json
{ "provider": "openai", "model": "gpt-6-luna", "reasoning": "auto", "modes": { "audit": { "reach": "read-only" } } }
```

## Sessions

Every run is saved as a session in `~/.llx/sessions/<id>.jsonl` (JSON Lines: the session's header, then each task as it ends, and its title); set `LLX_SESSIONS` to keep them elsewhere. A session records each task and how it ended, but not the conversation itself. `resume` therefore starts a new conversation that is told the earlier tasks, and the directory they ran in; at the terminal, the earlier tasks and their answers are drawn first (the last 20). In a session, each task continues the same conversation.

A session at the terminal runs in the `llx` process that shows it, with its agents and jobs: in its directory, with its environment and the options its command line gave. Quitting ends its work (asked first while something runs); `llx resume` or `llx -c` goes on with it later from its file. A session is open in one `llx` at a time: another that resumes it while it is open is refused (`session <id> is open in another llx: quit it there first`), and the file is left as it was.

`llx spend [ID]` says what a session spent (the newest of this directory without an ID): by task, by agent (an agent's turns are counted in the session that started it) and by model, with the share read from the prompt cache and what was written to it; then each miss of the cache with its likely cause and the dearest requests. `/usage` in a session ends with the same report. A miss is also noted as it happens (`✻ The prompt cache missed…`), with its cause and cost.

`LLX_RECORD=<dir>` keeps each session's run whole, for reading it again later (how the history was fitted, what an elision cost): `<dir>/<id>.jsonl`, a JSON line per event as it happens, the task's goal, each call and its whole output as the model read it (before any elision changed it), the model's words, each model turn's usage, each elision, the answer, an agent's events named by the agent. It is off by default; the file is yours alone (0600), and holds what the tools read, so keep the directory out of a workspace you share.

## Project instructions

llx reads instruction files, so a repository that already has them works as it is:

- your own, for every project: `~/.llx/AGENTS.md`;
- the project's: in each directory from the git repository's root down to the workspace, the first of `AGENTS.override.md`, `AGENTS.md` and `CLAUDE.md`, root first, so a deeper file wins over a shallower one;
- a directory below the workspace: its file is not in the context; the prompt tells the agent to read it before working there.

A line that is only `@path` brings that file in (relative to the file, or `~/…`), three levels deep, each file once, as in CLAUDE.md. The task you give wins over every file. Each file is capped at 16 KiB, and the project's files at 32 KiB together, with a note when something was cut. They reach the agent in a context block with the environment: the workspace, the OS, the date and the mode.

### The prompt cache

The context is built so the prompt cache keeps working. It is a message of its own after the system prompt, the same for every task and session in a workspace on a day (the date has no time), and it is marked for the cache, so a new session reads it from there. Files are read once per session; a subdirectory's file comes in as the result of the call that reads it; a mode change is a note appended later; a picture you attach stays. Nothing earlier in the conversation is rewritten turn by turn.
