# shopkit, 2026-11-10 03:08

A/B: 0.5.0 beside the output limit at 64k for every level, eager input streaming, a patch ended between files, an argument beside the text, a rewrite kept whole under a cut

69 tests; claude-haiku-5-5 (anthropic), reasoning low; llx's advisor (Jev) off; 1200 s a run, runs an agent: 1; Darwin arm64 26.6.2, load 6.1.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| base | llx | llx 0.5.0 | 2774144 | 27de67d95a08 |
| new | llx | llx 0.5.0 | 2774112 | 4f38fc9d364d |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 0 | yes | 69/69 | 62.4 | 62.3 | 0.1 | 4 | 0 | 0.0112 | 50 | 62233 | 15654 | 3 | 0 | 0 | 0 | 0 |
| new | 0 | yes | 69/69 | 67.3 | 66 | 1.3 | 6 | 0 | 0.0125 | 74 | 127623 | 16475 | 5 | 0 | 0 | 0 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
