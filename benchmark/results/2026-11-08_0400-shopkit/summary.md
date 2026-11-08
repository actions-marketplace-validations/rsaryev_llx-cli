# shopkit, 2026-11-08 04:00

llx against Claude Code, both on claude-sonnet-5-5 at effort low with the same Anthropic key

69 tests; claude-sonnet-5-5 (anthropic), reasoning low; llx's advisor (Jev) on; 1200 s a run, runs an agent: 1; Darwin arm64 26.6.2, load 6.1.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| llx | llx | b2f8ce14 | 2740832 | 2f8e52bada78 |
| claude | claude | 2.1.319 (Claude Code) | — | — |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| llx | 0 | yes | 69/69 | 48.2 | 47.4 | 0.8 | 5 | 0 | 0.1162 | 66 | 49200 | 7954 | 4 | 0 | 0 | 0 | 0 |
| claude | 0 | yes | 69/69 | 52.3 | — | — | 6 | — | 0.1149 | 69 | 45687 | 8380 | 5 | 0 | 0 | 0 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
