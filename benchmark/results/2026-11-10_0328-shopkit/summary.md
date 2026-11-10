# shopkit, 2026-11-10 03:28

llx (main after the output-limit fixes) beside Claude Code, both on Sonnet 5.5 with llx's Anthropic key, at the same minute

69 tests; claude-sonnet-5-5 (anthropic), reasoning low; llx's advisor (Jev) off; 1200 s a run, runs an agent: 2; Darwin arm64 26.6.2, load 7.8.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| llx | llx | 3ca3ebc2 | 2774112 | 4f38fc9d364d |
| claude | claude | 2.1.321 (Claude Code) | — | — |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| llx | 0 | yes | 69/69 | 48.5 | 48.4 | 0.1 | 4 | 0 | 0.1102 | 70 | 39717 | 8332 | 3 | 0 | 0 | 0 | 0 |
| claude | 0 | yes | 69/69 | 53 | — | — | 5 | — | 0.1057 | 64 | 31354 | 8117 | 4 | 0 | 0 | 0 | 0 |
| llx | 1 | yes | 69/69 | 42 | 41.9 | 0.1 | 4 | 0 | 0.0988 | 71 | 38770 | 7385 | 3 | 0 | 0 | 0 | 0 |
| claude | 1 | yes | 69/69 | 104.1 | — | — | 14 | — | 0.2445 | 89 | 193239 | 18298 | 14 | 4 | 0 | 0 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
