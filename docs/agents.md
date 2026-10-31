# llx in other agents

llx extends the agent you already use (Claude Code, another agent, a script of yours): it does not replace it. Your agent stays in charge, thinks, edits and checks; it calls llx from its own shell for what it lacks or what costs too much. Each block is a command, so the agent composes them as its task needs, and its own permission prompt shows every call.

| Block | What your agent gets | Command |
| --- | --- | --- |
| **helpers** | A whole task handed to an agent on any provider and model, in the background and in parallel, with an answer in your JSON shape | `llx exec` |
| **decide** | A quick, cheap choice about some text: yes or no, or one of your options | `llx decide` |
| **sessions** | The same helper again, with its conversation | `llx exec resume ID` |

## Helpers on any model

`llx exec` runs one task to its end and prints the answer alone on stdout. It thinks with any provider and model of the catalog, so your agent can hand bulk work to a cheaper one and keep its own tokens for what needs it:

```sh
llx exec --provider openrouter -m deepseek/deepseek-chat "List the public functions in src/ that have no test"
llx exec --schema @answer.json "Summarize the failing CI log in ci.log"
llx exec --allow 'bash(cargo test *)' "Run the tests and say which fail and why"
```

- With no rule it only reads: commands and file changes are refused. `--allow` and `--deny` say what it may do ([modes and rules](modes.md)).
- `--json` prints its events; the last, `run.completed`, has the answer, the session id, the cost and the time. `exec resume ID "…"` goes on with the same helper.
- Several at once: start each in the background and read their answers when they end.
- Check what a helper says before you act on it, as you would a colleague's.

## decide

`llx decide` asks llx's decision model one question about stdin and prints the option it picked, in about a second:

```sh
git diff | llx decide "Does this change touch the public API?"           # yes or no
llx decide --choices patch,minor,major "Which version bump?" < CHANGES.md
```

It needs an OpenRouter key. Its options are in the [reference](reference.md#llx-decide).
