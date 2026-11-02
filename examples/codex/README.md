# llx in Codex

Codex reads its instructions from `AGENTS.md`. Paste [llx.md](llx.md) into your `AGENTS.md` (the
repository's, or `~/.codex/AGENTS.md` for every project): Codex then hands bulk reading to
`llx exec` on another model, and asks `llx decide` for the judgments that would cost it a long
read.

- Keep the `llx decide` item only where `llx decide --check` exits 0: it needs an OpenRouter key
  (`OPENROUTER_API_KEY`, or one saved with `/login` in llx).
- llx needs the network and sets up its own sandbox, so approve its commands outside Codex's
  (or let them run there with Codex's network access on).
- The same lines work for any agent that reads `AGENTS.md`.

For Claude Code there is a plugin: [claude-code/](../claude-code/README.md).
