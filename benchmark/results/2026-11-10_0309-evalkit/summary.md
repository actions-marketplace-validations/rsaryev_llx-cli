# evalkit, 2026-11-10 03:09

A/B: 0.5.0 beside the output limit at 64k for every level, eager input streaming, a patch ended between files, an argument beside the text, a rewrite kept whole under a cut

238 tests; claude-haiku-5-5 (anthropic), reasoning low; llx's advisor (Jev) off; 1200 s a run, runs an agent: 3; Darwin arm64 26.6.2, load 3.7.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| base | llx | llx 0.5.0 | 2774144 | 27de67d95a08 |
| new | llx | llx 0.5.0 | 2774112 | 4f38fc9d364d |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 0 | yes | 238/238 | 354.3 | 353.7 | 0.6 | 14 | 0 | 0.1340 | 86 | 978268 | 88760 | 13 | 1 | 0 | 0 | 0 |
| new | 0 | yes | 238/238 | 246.6 | 244.8 | 1.8 | 13 | 0 | 0.0447 | 88 | 670891 | 61681 | 12 | 0 | 0 | 0 | 0 |
| base | 1 | yes | 238/238 | 477.9 | 476.4 | 1.5 | 21 | 0 | 0.1716 | 93 | 1805130 | 116139 | 23 | 0 | 8 | 1 | 0 |
| new | 1 | yes | 238/238 | 268.8 | 268 | 0.8 | 16 | 0 | 0.0516 | 90 | 979438 | 66748 | 15 | 0 | 0 | 0 | 0 |
| base | 2 | yes | 238/238 | 493 | 492.4 | 0.6 | 19 | 0 | 0.1831 | 93 | 1780338 | 116728 | 17 | 2 | 2 | 0 | 0 |
| new | 2 | yes | 238/238 | 285.1 | 284.6 | 0.5 | 9 | 0 | 0.0477 | 81 | 455300 | 70620 | 8 | 0 | 0 | 0 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
