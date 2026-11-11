# evalkit, 2026-11-10 02:53

A/B/C: 0.5.0, the three fixes (End Patch between files, an argument beside the text, eager input streaming), and the fixes with the patch rule

238 tests; claude-haiku-5-5 (anthropic), reasoning low; llx's advisor (Jev) off; 1200 s a run, runs an agent: 2; Darwin arm64 26.6.2, load 5.9.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| base | llx | llx 0.5.0 | 2774144 | 27de67d95a08 |
| fix | llx | llx 0.5.0 | 2774096 | 7b68b16a70eb |
| fixrule | llx | llx 0.5.0 | 2774096 | 25168227afc4 |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 0 | yes | 238/238 | 256.3 | 255.6 | 0.7 | 9 | 0 | 0.0433 | 80 | 396606 | 64126 | 8 | 0 | 0 | 0 | 0 |
| fix | 0 | yes | 238/238 | 250.3 | 249.3 | 1 | 11 | 0 | 0.0438 | 84 | 529413 | 61367 | 10 | 1 | 0 | 0 | 0 |
| fixrule | 0 | yes | 238/238 | 295.1 | 294.4 | 0.7 | 7 | 0 | 0.0493 | 73 | 347011 | 75062 | 10 | 0 | 4 | 0 | 0 |
| base | 1 | yes | 238/238 | 543.5 | 542.5 | 1 | 18 | 0 | 0.2672 | 91 | 1632867 | 131127 | 16 | 1 | 0 | 0 | 0 |
| fix | 1 | yes | 238/238 | 247.9 | 247.3 | 0.6 | 8 | 0 | 0.0411 | 77 | 338488 | 61305 | 7 | 0 | 0 | 0 | 0 |
| fixrule | 1 | yes | 238/238 | 498.9 | 498 | 0.9 | 21 | 0 | 0.2613 | 93 | 1985592 | 120966 | 28 | 0 | 13 | 1 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
