# llx and other agents' GitHub Actions

What `rsaryev/llx` does next to the agent actions most teams look at first:
[anthropics/claude-code-action](https://github.com/anthropics/claude-code-action) and
[openai/codex-action](https://github.com/openai/codex-action), as their repositories stood on
2026-11-01. Each is good at what it sets out to do; this page says where they differ, so you can
pick by what your workflow needs.

## At a glance

| | llx | claude-code-action | codex-action |
| --- | --- | --- | --- |
| What runs | `llx exec`: one binary under 4 MB | Claude Code through its Agent SDK, on Bun | `codex exec`, from npm, on Node |
| Install | The release archive, checked against `SHA256SUMS` and attested | `curl … install.sh \| bash` of a version pinned in the action | `npm install -g @openai/codex` (the latest, unless `codex-version`) |
| Models | 1,000+ from 200+ providers: OpenRouter, OpenAI, Anthropic, DeepSeek, any key the [catalog](../MODELS.md) names | Claude: the Anthropic API, Bedrock, Vertex, Foundry, with OIDC | OpenAI's, or an Azure endpoint |
| How it reaches GitHub | `gh` and `git` in its shell, with the job's token | MCP servers for comments, inline reviews, commits and CI logs, and wrappers around `git push` and `gh` | Its shell; the workflow posts |
| Who writes to GitHub | A second job with no model key: the [two-job pattern](ci.md#the-two-job-pattern) | The agent, with an app token that can write | The workflow, from `final-message` |
| The answer | `result` (data matching `schema`), `result-file`, `exit-code`, `status`, `cost-usd`, `turns`, `events-file` | `structured_output`, `conclusion`, `execution_file`, `session_id`, `branch_name` | `final-message` |
| Limits | `max-turns`, `timeout` (always set), `budget` in dollars | Through `claude_args` (`--max-turns`) | The job's timeout |
| Triggers | Yours: any event; the examples answer `@llx`, review, triage, CI failures, Jira | A built-in `@claude` mode with a progress comment, branches and links; or a prompt of yours | Yours |

## The provider's key

A command the agent runs can try to read the key out of the agent's process. All three take it
seriously, in different ways:

- **llx** removes the key from every command's variables and is non-dumpable on Linux, so a
  process of the same user cannot read its memory or `/proc/<pid>/environ`. On a hosted Linux
  runner the action also drops `sudo` and the runner's groups (Docker's) for llx and everything
  it runs (`privileges`), so nothing can become root to read it.
- **codex-action** keeps the key out of Codex: a local proxy holds it and forwards Codex's
  requests, and its `drop-sudo` strategy removes the runner's `sudo` (for the rest of the job)
  and denies it the root services' sockets.
- **claude-code-action**, when users without write access may trigger it
  (`allowed_non_write_users`), scrubs the variables of the commands it runs and runs them in
  bubblewrap.

## Untrusted input

- **Who may trigger it.** claude-code-action and codex-action check GitHub's permission API for
  write access (codex-action: `allow-users`, `allow-bots`; claude-code-action: `allowed_bots`,
  `allowed_non_write_users`). llx's [llx.yml](../examples/github/llx.yml) asks the same API; the
  action itself leaves triggers to your workflow.
- **Instruction files on a pull request.** claude-code-action puts its configuration files back
  as the base branch has them. llx does it for `AGENTS.md`, `AGENTS.override.md` and `CLAUDE.md`
  with `instructions-from: base`.
- **Text written by others.** claude-code-action strips hidden markup from comments and leaves
  out those edited after the trigger. llx's examples hand the agent such text as files or `gh`
  output, never as the prompt, and say it is data; `llx.yml` gives it the request as the event
  carried it.
- **A secret in the answer.** claude-code-action redacts tokens in what it posts; llx's action
  replaces any value of the step's secret variables in the answer and the events with `***`.

## What llx leaves out, on purpose

No MCP server, no GitHub tool, no built-in prompt and no progress comment the agent edits as it
works: the agent has a shell, and the workflow owns the prompt and what is posted, where you can
read both ([docs/ci.md](ci.md)). The same prompt then runs on GitHub, GitLab or any runner with a
shell. If you want the agent itself to hold a write token and drive a pull request through
comments, claude-code-action is built for that.
