# llx in Claude Code

A Claude Code plugin. Your session keeps thinking on its own model; llx adds a search of the code
by what a question means (`llx grep`), fast, cheap judgments about long or many texts
(`llx decide`), and a subagent that hands bulk reading to llx on the provider and model you chose
with `/login` in llx (a ChatGPT sign-in, OpenRouter, a key of your own).

## Install

llx first (`curl -fsSL https://llx.sh/install | sh`, then `llx` and `/login`), then in Claude Code
2.1.275 or later:

```text
/plugin install llx --marketplace rsaryev/llx
```

On an older one: `/plugin marketplace add rsaryev/llx`, then `/plugin install llx@llx`. As the
plugin is enabled, Claude Code asks for its settings, all optional: an **OpenRouter key** (kept in
the system keychain; `llx grep` and `llx decide` run on OpenRouter's decision model, and the key
reaches only llx's commands, never another program Claude runs), and the **provider** and
**model** the subagent's llx thinks with (empty: what `/login` in llx chose). Change them later
with `claude plugin configure llx`. A key or a setting the environment already has wins. Start a
new session: plugins, skills and the session's hook are read as it starts.

A skill allows its commands while it is loaded. To have Claude run them without a prompt before
it loads one, allow them once in `.claude/settings.json` or `~/.claude/settings.json` (they
change nothing, and the subagent runs `llx exec` read-only):

```json
{ "permissions": { "allow": ["Bash(llx grep *)", "Bash(llx decide *)", "Bash(llx exec *)"] } }
```

## What it adds

| Part | What it does |
| --- | --- |
| [skills/grep](skills/grep/SKILL.md) | `llx:grep`: when to search with `llx grep` (a question about where or how something is done, with no exact name), and the commands it allows. Its body sends Claude to `llx decide --check` and `llx grep --help` |
| [skills/decide](skills/decide/SKILL.md) | `llx:decide`: when a judgment with a few options about a long text, many files or an untrusted text is worth `llx decide`; its body sends Claude to `llx decide --help` |
| [agents/llx.md](agents/llx.md) | The `llx` subagent: a small Claude model (`haiku`, three turns at most) that hands a task to `llx exec --mode plan` and relays its answer. A Claude Code subagent always thinks on a Claude model; here it only relays, and llx's model does the work |
| [hooks/session-start.sh](hooks/session-start.sh) | As a session starts, hands the plugin's settings to llx in Claude's commands, and, where llx is installed, says in one line to load the skills for such questions: a skill's description alone seldom turns Claude from its own Grep |

The skills are pointers, not guides, as [agent-browser](https://github.com/vercel-labs/agent-browser)'s
are: what `llx grep` and `llx decide` do, when they pay and how to read their output is in their
`--help`, the installed llx's own, so it never goes stale between releases. `llx exec --help`,
`llx grep --help` and `llx decide --help` each have a paragraph for an agent.

## Evals

[evals/run.ts](evals/run.ts) asks Claude Code (`claude -p`) questions about another project's
code ([evals/cases.json](evals/cases.json): agent-browser at a fixed commit, its answers known),
once with this plugin (`--plugin-dir`) and once without, the user's own settings and plugins left
out. It records which skills Claude loaded, whether it read the `--help` before it first ran
`llx grep` or `llx decide`, whether it used what the case expects (a question whose words are not
the code's: `llx grep`; an exact name: neither), how many of the files it had to name it named,
and its turns, time and cost:

```sh
cd examples/claude-code/evals && bun run.ts            # every case, both arms
bun run.ts --only c4 --arm plugin                      # one case, with the plugin
```

On the first run (Claude Opus 5.5, one run a case), Claude named every file the answers need
with the plugin and without, at about the same cost ($0.62 and $0.55 for the four); it loaded
`llx:grep` once and ran `llx grep` once, which a missing allow rule refused. A strong model
turns a question into the code's words well on a project of this size, so the search pays where
it cannot: a large codebase, names it cannot guess. Each run's transcripts stay in
`evals/results/` (ignored by git).

## What it costs

A search is one request to the decision model for each file it reads, about half a cent in all; a
decision is one, a fraction of a cent. The subagent's relay is a few Haiku turns, about $0.0015 a
task; the task itself is llx's, on your sign-in. Claude checks a helper's answer as it would a
colleague's, so on a small task the saving is nothing: it pays where the reading is large.

## Change it

Edit the files: the skills' descriptions say when Claude loads them, the subagent's frontmatter
its model and limits. Run the evals after a change: they show whether Claude still does what the
cases expect.
