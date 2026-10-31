# evalkit, 2026-10-31 22:13

Codex's first evalkit run, beside llx's with Jev of the same evening.

238 tests; gpt-6-luna (openai), reasoning low; llx's advisor (Jev) on; 1200 s a run, runs an agent: 1; Darwin arm64 26.6.2, load 6.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| codex | codex | codex-cli 0.163.0 | — | — |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| codex | 0 | no | 197/238 | 148.5 | — | — | 23 | — | 0.0248 | 89 | 905504 | 14227 | 20 | 3 | 6 | 0 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
