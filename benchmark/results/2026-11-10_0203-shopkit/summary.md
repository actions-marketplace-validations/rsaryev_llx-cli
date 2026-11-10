# shopkit, 2026-11-10 02:03

llx beside Claude Code, both on Haiku 5.5 with llx's Anthropic key, at the same minute

69 tests; claude-haiku-5-5 (anthropic), reasoning low; llx's advisor (Jev) off; 1200 s a run, runs an agent: 1; Darwin arm64 26.6.2, load 14.7.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| llx | llx | f0b338c5 | 2774144 | b4d47a512be0 |
| claude | claude | 2.1.321 (Claude Code) | — | — |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| llx | 0 | yes | 69/69 | 69.1 | 68.9 | 0.2 | 5 | 0 | 0.0118 | 71 | 95406 | 16630 | 4 | 0 | 0 | 0 | 0 |
| claude | 0 | yes | 69/69 | 67.1 | — | — | 4 | — | 0.0110 | 43 | 50055 | 15963 | 3 | 0 | 0 | 0 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
