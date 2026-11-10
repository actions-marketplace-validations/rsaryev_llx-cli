# shopkit, 2026-11-10 02:52

A/B/C: 0.5.0, the three fixes (End Patch between files, an argument beside the text, eager input streaming), and the fixes with the patch rule

69 tests; claude-haiku-5-5 (anthropic), reasoning low; llx's advisor (Jev) off; 1200 s a run, runs an agent: 1; Darwin arm64 26.6.2, load 3.2.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| base | llx | llx 0.5.0 | 2774144 | 27de67d95a08 |
| fix | llx | llx 0.5.0 | 2774096 | 7b68b16a70eb |
| fixrule | llx | llx 0.5.0 | 2774096 | 25168227afc4 |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 0 | yes | 69/69 | 56.4 | 56.1 | 0.3 | 6 | 0 | 0.0110 | 70 | 102985 | 14490 | 5 | 0 | 0 | 0 | 0 |
| fix | 0 | yes | 69/69 | 69.1 | 68.9 | 0.2 | 6 | 0 | 0.0127 | 69 | 110989 | 16923 | 5 | 0 | 0 | 0 | 0 |
| fixrule | 0 | yes | 69/69 | 66.5 | 66.4 | 0.1 | 4 | 0 | 0.0121 | 49 | 64221 | 17035 | 5 | 0 | 2 | 0 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
