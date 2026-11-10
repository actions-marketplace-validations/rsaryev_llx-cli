# shopkit, 2026-11-10 01:55

Haiku 5.5 over Anthropic's Messages API, after the 2026-11-10 cleanup

69 tests; claude-haiku-5-5 (anthropic), reasoning low; llx's advisor (Jev) off; 1200 s a run, runs an agent: 1; Darwin arm64 26.6.2, load 17.2.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| llx | llx | 5eef02d7 | 2774144 | b4d47a512be0 |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| llx | 0 | yes | 69/69 | 65.5 | 65.4 | 0.1 | 4 | 0 | 0.0114 | 57 | 63771 | 16531 | 3 | 0 | 0 | 0 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
