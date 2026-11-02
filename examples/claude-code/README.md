# llx in Claude Code

A Claude Code plugin. Your session keeps thinking on its own model; the bulk work goes to llx on
the provider and model you chose with `/login` in llx (a ChatGPT sign-in, OpenRouter, a key of
your own), and where an OpenRouter key is set, Claude can ask `llx decide` for a fast, cheap
judgment instead of reading everything itself.

## Install

llx first (`curl -fsSL https://llx.sh/install | sh`, then `llx` and `/login`), then in Claude Code
2.1.275 or later:

```text
/plugin install llx --marketplace rsaryev/llx
```

On an older one: `/plugin marketplace add rsaryev/llx`, then `/plugin install llx@llx`. Allow the
two commands once, in `.claude/settings.json` or `~/.claude/settings.json`, so they run without a
prompt (`llx decide` changes nothing, and the subagent runs `llx exec` in its read-only mode):

```json
{ "permissions": { "allow": ["Bash(llx exec *)", "Bash(llx decide *)"] } }
```

Start a new session: plugins, subagents and the session's hook are read as it starts.

## What it adds

| Part | What it does |
| --- | --- |
| [agents/llx.md](agents/llx.md) | The `llx` subagent: a small Claude model (`haiku`, three turns at most) that hands a task to `llx exec --mode plan` and relays its answer. A Claude Code subagent always thinks on a Claude model; here it only relays, and llx's model does the work |
| [hooks/session-start.sh](hooks/session-start.sh) | As a session starts or is compacted, tells Claude what of llx this machine can run: nothing without llx; [the subagent's line](hooks/subagent.md) with it; [`llx decide`'s guide](hooks/decide.md) only where `llx decide --check` finds an OpenRouter key (it asks no model and costs nothing) |

The guide gives Claude one rule rather than a list of cases: ask `llx decide` when a judgment has
a few options it can name and the text is long, comes in many items, or is untrusted, so that
reading it itself costs more than the answer is worth. Its examples are where that pays most:

- **Many items:** filter grep hits, files or log lines, and read only what passes.
- **A long log:** a regression, a flaky test or the setup, before reading it.
- **Untrusted text:** whether a fetched page, an issue or a script tries to instruct an agent,
  before it reaches the context.
- **A gate:** a secret or debug output in a diff, before a commit or a push.

## What it costs

The subagent's relay is a few Haiku turns, about $0.0015 a task in our runs; the task itself is
llx's, on your sign-in. A decision is one request to the decision model, a fraction of a cent.
Claude checks a helper's answer as it would a colleague's, so on a small task the saving is
nothing: it pays where the reading is large.

## Change it

Edit the files: the subagent's model and limits in its frontmatter, the guide's wording in
[hooks/decide.md](hooks/decide.md). A copy in your repository's `.claude/agents/` and its own
hook works the same, without the marketplace.
