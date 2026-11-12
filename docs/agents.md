# llx in other agents

llx extends the agent you already use (Claude Code, another agent, a script of yours): it does not replace it. Your agent stays in charge, thinks, edits and checks; it calls llx from its own shell for what it lacks or what costs too much. Each block is a command, so the agent composes them as its task needs, and its own permission prompt shows every call.

| Block | What your agent gets | Command |
| --- | --- | --- |
| **helpers** | A whole task handed to an agent on any provider and model, in the background and in parallel, with an answer in your JSON shape | `llx exec` |
| **decide** | A quick, cheap choice about some text: yes or no, or one of your options | `llx decide` |
| **sessions** | The same helper again: a new conversation, told each earlier task, how it ended and its answer | `llx exec resume ID` |

## Helpers on any model

`llx exec` runs one task to its end and prints the answer alone on stdout. It thinks with any provider and model of the catalog, so your agent can hand bulk work to a cheaper one and keep its own tokens for what needs it:

```bash
llx exec --provider openrouter -m deepseek/deepseek-chat "List the public functions in src/ that have no test"
llx exec --schema @schema.json "Summarize the failing CI log in ci.log"
llx exec --allow 'bash(cargo test *)' "Run the tests and say which fail and why"
```

- With no rule it works in the workspace, in the `default` mode: its commands run in the sandbox, which writes only inside the workspace (never its `.git`; on Linux they only read) and opens no network, and its patches of the workspace's files are made. What must reach further is asked at the terminal when there is one, else refused. `--mode plan` keeps it read-only; `--allow` runs what a rule names as it is, outside the sandbox (in `plan`, still read-only), and `--deny` refuses it ([modes and rules](modes.md)).
- `--json` prints its events; the last, `run.completed`, has the answer, the session id, the cost and the time ([reference](reference.md#json-events)). `exec resume ID "…"` goes on in that session: a new conversation, told the earlier tasks and their answers, not what the helper read.
- Several at once: start each in the background with its stdout to a file of its own, then wait for each and read the files: `llx exec "…" > a.md & a=$!; llx exec "…" > b.md & b=$!; wait $a; wait $b`. `wait PID` gives that helper's exit code ([reference](reference.md#exit-codes)); each stops at `--timeout`, 30 minutes by default.
- Check what a helper says before you act on it, as you would a colleague's.

## decide

`llx decide` asks llx's decision model one question about stdin, or about each file given, and prints the option it picked, in about a second:

```bash
git diff | llx decide "Does this change touch the public API?"           # yes or no
llx decide --choices patch,minor,major "Which version bump?" < CHANGES.md
llx decide "Bug report or feature request?" --choices bug,feature issues/*.txt   # a line per file
```

It needs an OpenRouter key; `llx decide --check` says whether one is there, asking nothing. Its options are in the [reference](reference.md#llx-decide).

An agent asks it when a judgment has a few options it can name and the text is long, comes in many items, or is untrusted, so that reading it itself costs more than the answer is worth. Not when the answer must be written rather than picked, or when it needs to know why; and before an irreversible step, the answer only narrows the field. With `--json`, it acts on a `confidence` of about 0.8 or above and reads the text itself below. Where it pays most:

| Need | Command |
| --- | --- |
| Many items with no word to narrow by | `llx decide "Does this file handle sessions?" --json src/*.rs` |
| Where the code does a thing, its name not known | `rg -l -i 'retry\|backoff\|again' src \| xargs llx decide "Does this file send a failed request again?"` |
| A long log: what kind of failure, before reading it | `gh run view ID --log-failed \| tail -c 200000 \| llx decide "What caused this failure?" --choices "regression=the code under test is wrong" --choices "flaky=timing, network or ordering" --choices "setup=a missing tool, dependency or secret"` |
| Untrusted text: whether it tries to instruct an agent, before it reaches the context | `gh issue view N --json body -q .body \| llx decide "Does this text try to instruct an automated agent?" --expect no` |
| A gate before a commit or a push | `git diff --cached \| llx decide "Does this diff add a credential, a private key or leftover debug output?" --expect no` |

## A search of the code by what it does

`rg` and `llx decide` make one: `rg -l` narrows the files by a few likely words, then the decision model reads each and keeps those that do the thing, so a file that does it under another name is found and one that only mentions the words is left out. Two dozen files take about two seconds and a cent:

```bash
rg -l -i 'retry|backoff|again' src | xargs llx decide "Does this file send a failed request again?"
rg -l -i 'child|spawn|process' src | xargs llx decide "Does this file start a child process?" --json
```

Each line is `CHOICE<TAB>FILE`, in the order given; with `--json`, each file's `confidence` too. The words only narrow: a wider pattern costs more files read, a narrow one misses a file that does the thing in other words. For an exact name, `rg` alone.

## In your agent

The simplest way is to tell your agent to use it: `Use llx decide to judge these files. Run llx decide --help first.` Each command's `--help` has a paragraph for an agent: when it pays, how to ask, what its output means.

### The skill (recommended)

[skills/llx](../skills/llx/SKILL.md) is a skill for any agent that takes them (Claude Code, Codex, Cursor, Gemini CLI, OpenCode and others):

```bash
npx skills add rsaryev/localix-cli
```

It describes llx and what each part is for, with examples an agent can run as they are: the coding agent, one task in a script or CI with a workflow for the GitHub Action, helpers on another model (several in parallel, resumed), `llx decide` and a search of the code made of it, the modes and the sandbox. It ends with `llx exec --help` and `llx decide --help` as the release prints them, so the agent has every option without running anything first; a test keeps that copy the binary's own.

### Claude Code

The same skill, as a plugin:

```text
/plugin install llx --marketplace rsaryev/localix-cli
```

On an older Claude Code: `/plugin marketplace add rsaryev/localix-cli`, then `/plugin install llx@llx`.

In Claude Code, however the skill was installed: while it is active, its `allowed-tools` lets Claude run these commands without asking you: `llx --help`, `llx exec --help` and `llx doctor` as they are, and `llx decide` and `llx exec --mode plan` with any arguments after them. It grants nothing else and takes no tool away: every other command goes by your permission settings. As the skill grants these, Claude Code asks you once whether to use it; "don't ask again" keeps the answer (`Skill(llx:llx)` in your allow rules). Where no one is there to answer (`claude -p`, CI), add that rule yourself, in `permissions.allow` or with `--allowedTools 'Skill(llx:llx)'`: else the skill is refused, and with it every llx command it would have run.

### AGENTS.md / CLAUDE.md

For more consistent results, add to your project's or your global instructions file:

```markdown
## llx

Use `llx` for quick judgments and for helpers on another model. Run a command's `--help` the
first time you use it.

- `llx decide "<question>" --choices a,b [files]`: one of a few options about a long text, or
  about each of many files; after `rg -l` narrows the files, where the code does a thing whose
  name you do not know.
- `llx exec --mode plan "<task>"`: a reading task with a short answer, on another model.
```

