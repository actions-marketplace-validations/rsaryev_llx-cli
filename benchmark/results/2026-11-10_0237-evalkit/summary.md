# evalkit, 2026-11-10 02:37

A/B: the prompt's rule to change files with a patch, not a command

238 tests; claude-haiku-5-5 (anthropic), reasoning low; llx's advisor (Jev) off; 1200 s a run, runs an agent: 2; Darwin arm64 26.6.2, load 9.3.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| base | llx | llx 0.5.0 | 2774144 | 27de67d95a08 |
| patch | llx | llx 0.5.0 | 2774144 | bed1a37fc940 |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 0 | yes | 238/238 | 285.7 | 284.9 | 0.8 | 9 | 0 | 0.0484 | 82 | 491211 | 71243 | 8 | 0 | 0 | 0 | 0 |
| patch | 0 | yes | 238/238 | 556.8 | 556.3 | 0.5 | 16 | 0 | 0.2506 | 90 | 1554123 | 135984 | 22 | 1 | 10 | 2 | 0 |
| base | 1 | yes | 238/238 | 344.2 | 341.5 | 2.7 | 20 | 0 | 0.1161 | 93 | 1519648 | 83514 | 19 | 1 | 5 | 0 | 0 |
| patch | 1 | yes | 238/238 | 535.1 | 534.4 | 0.7 | 19 | 0 | 0.2658 | 92 | 1760496 | 130381 | 23 | 0 | 12 | 1 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
