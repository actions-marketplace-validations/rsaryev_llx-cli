# evalkit, 2026-11-02 22:38

after the daemon's removal, JSONL sessions, the seam's cleanup

238 tests; gpt-6-luna (openai), reasoning low; llx's advisor (Jev) on; 1200 s a run, runs an agent: 1; Darwin arm64 26.6.2, load 1.7.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| llx | llx | 55514bfe | 2391200 | 60a2cd3167af |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| llx | 0 | yes | 238/238 | 1094.7 | 359.5 | 735.2 | 83 | 1 | 0.1108 | 98 | 8108586 | 32397 | 88 | 20 | 51 | 9 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
