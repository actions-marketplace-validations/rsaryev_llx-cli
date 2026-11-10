# shopkit, 2026-11-10 02:36

A/B: the prompt's rule to change files with a patch, not a command

69 tests; claude-haiku-5-5 (anthropic), reasoning low; llx's advisor (Jev) off; 1200 s a run, runs an agent: 2; Darwin arm64 26.6.2, load 10.4.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| base | llx | llx 0.5.0 | 2774144 | 27de67d95a08 |
| patch | llx | llx 0.5.0 | 2774144 | bed1a37fc940 |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 0 | yes | 69/69 | 56.3 | 56.1 | 0.2 | 5 | 0 | 0.0109 | 67 | 91026 | 14559 | 4 | 0 | 0 | 0 | 0 |
| patch | 0 | yes | 69/69 | 86 | 85.8 | 0.2 | 6 | 0 | 0.0171 | 72 | 142469 | 24214 | 5 | 0 | 2 | 1 | 0 |
| base | 1 | yes | 69/69 | 55 | 54.8 | 0.2 | 5 | 0 | 0.0106 | 67 | 90606 | 13920 | 4 | 0 | 0 | 0 | 0 |
| patch | 1 | yes | 69/69 | 73.3 | 73.2 | 0.1 | 6 | 0 | 0.0149 | 76 | 134944 | 21258 | 5 | 0 | 2 | 1 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
