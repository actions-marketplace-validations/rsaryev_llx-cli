# shopkit, 2026-10-31 16:15

Before the cleanup: the first calibration of shopkit.

69 tests; gpt-6-luna (openai), reasoning low; llx's advisor (Jev) off; 900 s a run, runs an agent: 1; Darwin arm64 26.6.2, load ?.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| base | llx | e554ec72 | 2855744 | e3fd7d9af5b0 |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 0 | yes | 69/69 | 39.7 | 39.6 | 0.1 | 5 | 0 | 0.0043 | 68 | 48862 | 4778 | 6 | 0 | 1 | 0 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
