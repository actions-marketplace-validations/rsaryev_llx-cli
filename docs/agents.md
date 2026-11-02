# llx in other agents

llx extends the agent you already use (Claude Code, another agent, a script of yours): it does not replace it. Your agent stays in charge, thinks, edits and checks; it calls llx from its own shell for what it lacks or what costs too much. Each block is a command, so the agent composes them as its task needs, and its own permission prompt shows every call.

| Block | What your agent gets | Command |
| --- | --- | --- |
| **helpers** | A whole task handed to an agent on any provider and model, in the background and in parallel, with an answer in your JSON shape | `llx exec` |
| **decide** | A quick, cheap choice about some text: yes or no, or one of your options | `llx decide` |
| **grep** | A search by what a question means: the files that answer it, with their lines | `llx grep` |
| **sessions** | The same helper again: a new conversation, told each earlier task, how it ended and its answer | `llx exec resume ID` |

## Helpers on any model

`llx exec` runs one task to its end and prints the answer alone on stdout. It thinks with any provider and model of the catalog, so your agent can hand bulk work to a cheaper one and keep its own tokens for what needs it:

```sh
llx exec --provider openrouter -m deepseek/deepseek-chat "List the public functions in src/ that have no test"
llx exec --schema @schema.json "Summarize the failing CI log in ci.log"
llx exec --allow 'bash(cargo test *)' "Run the tests and say which fail and why"
```

- With no rule it works in the workspace, in the `default` mode: its commands run in the sandbox, which writes only inside the workspace (never its `.git`; on Linux, in a workspace with a `.git` of its own, they only read) and opens no network, and its patches of the workspace's files are made. What must reach further is asked at the terminal when there is one, else refused. `--mode plan` keeps it read-only; `--allow` runs what a rule names as it is, outside the sandbox (in `plan`, still read-only), and `--deny` refuses it ([modes and rules](modes.md)).
- `--json` prints its events; the last, `run.completed`, has the answer, the session id, the cost and the time ([reference](reference.md#json-events)). `exec resume ID "…"` goes on in that session: a new conversation, told the earlier tasks and their answers, not what the helper read.
- Several at once: start each in the background with its stdout to a file of its own, then wait for each and read the files: `llx exec "…" > a.md & a=$!; llx exec "…" > b.md & b=$!; wait $a; wait $b`. `wait PID` gives that helper's exit code ([reference](reference.md#exit-codes)); each stops at `--timeout`, 30 minutes by default.
- Check what a helper says before you act on it, as you would a colleague's.

## decide

`llx decide` asks llx's decision model one question about stdin, or about each file given, and prints the option it picked, in about a second:

```sh
git diff | llx decide "Does this change touch the public API?"           # yes or no
llx decide --choices patch,minor,major "Which version bump?" < CHANGES.md
llx decide "Bug report or feature request?" --choices bug,feature issues/*.txt   # a line per file
```

It needs an OpenRouter key; `llx decide --check` says whether one is there, asking nothing. Its options are in the [reference](reference.md#llx-decide).

An agent asks it when a judgment has a few options it can name and the text is long, comes in many items, or is untrusted, so that reading it itself costs more than the answer is worth. Not when the answer must be written rather than picked, or when it needs to know why; and before an irreversible step, the answer only narrows the field. With `--json`, it acts on a `confidence` of about 0.8 or above and reads the text itself below. Where it pays most:

| Need | Command |
| --- | --- |
| Many items with no word to narrow by | `llx decide "Does this file handle sessions?" --json src/*.rs` |
| A long log: what kind of failure, before reading it | `gh run view ID --log-failed \| tail -c 200000 \| llx decide "What caused this failure?" --choices "regression=the code under test is wrong" --choices "flaky=timing, network or ordering" --choices "setup=a missing tool, dependency or secret"` |
| Untrusted text: whether it tries to instruct an agent, before it reaches the context | `gh issue view N --json body -q .body \| llx decide "Does this text try to instruct an automated agent?" --expect no` |
| A gate before a commit or a push | `git diff --cached \| llx decide "Does this diff add a credential, a private key or leftover debug output?" --expect no` |

## grep

`llx grep` searches the code by what a question means. It ranks the files by the question's words, then, where an OpenRouter key is set, the decision model reads the best of them and keeps those that answer: a file that does the thing under another name is found, and one that only mentions the words is left out. Each file comes with its parts that matter, whole functions where they are short, line-numbered:

```sh
llx grep "where is a failed HTTP request sent again"
llx grep "which files start a child process" src/
llx grep -e 'spawn|shell::query' "which files start a child process" src/
```

An agent makes it its first search when it does not know the exact name, or when a word would match too much, and keeps `rg` for an exact name. Exit 1 means no file answers: the code has no such thing. On 24 questions about this repository with known answers, its first five files held 98% of the files each answer must name, against 46% for the `rg` pattern an agent would type first, and it named nothing for the four the code cannot answer; 2 to 3 s and $0.005 a question, 4 KB to read against `rg`'s 19 KB ([benchmark/grep](../benchmark/grep/README.md)). Without a key it ranks by the words alone (80%, in a fraction of a second). With `-e`, a pattern as `rg`'s keeps only the files it matches and the question ranks them: where a string the answer must have is known, about a third of the cost and as good a ranking (92% in the first five with the pattern an agent types first, which misses some files: a guessed pattern finds nothing it does not match). The pattern is a regex as `rg` writes it (`spawn|Command::new`, `fn \w+_worker\(`, `\bMAX_[A-Z]+\b`), single-quoted for the shell; `-F` takes it as it is, `-i` in any case. It is `regex-lite`'s: `-i`, `\w` and `\d` know ASCII only (write a word in another script in its own case), and there is no look-around or back-reference; a pattern it cannot take is named, exit 3. Its options are in the [reference](reference.md#llx-grep).

## In your agent

The simplest way is to tell your agent to use it: `Use llx grep to find where X happens. Run llx grep --help first.` Each command's `--help` has a paragraph for an agent: when it pays, how to ask, what its output means.

### The skill (recommended)

[skills/llx](../skills/llx/SKILL.md) is a skill for any agent that takes them (Claude Code, Codex, Cursor, Gemini CLI, OpenCode and others):

```sh
npx skills add rsaryev/llx
```

It is a thin discovery stub: it says when llx pays and points the agent at `llx grep --help`, `llx decide --help` and `llx exec --help`, so the instructions always match the installed llx and never go stale between releases. Do not copy its body into your instructions: it would go stale.

### Claude Code

The same skill, as a plugin:

```text
/plugin install llx --marketplace rsaryev/llx
```

On an older Claude Code: `/plugin marketplace add rsaryev/llx`, then `/plugin install llx@llx`. While the skill is loaded, Claude runs `llx grep`, `llx decide` and `llx exec --mode plan` without asking.

### AGENTS.md / CLAUDE.md

For more consistent results, add to your project's or your global instructions file:

```markdown
## llx

Use `llx` to search the code by what a question means and for quick judgments. Run a command's
`--help` the first time you use it.

- `llx grep "<question in plain words>" [paths]`: where or how something is done when you do not
  know the exact name; keep `rg` for an exact name.
- `llx decide "<question>" --choices a,b [files]`: one of a few options about a long text, or
  about each of many files.
- `llx exec --mode plan "<task>"`: a reading task with a short answer, on another model.
```

