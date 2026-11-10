# evalkit, 2026-11-10 02:04

llx beside Claude Code, both on Haiku 5.5 with llx's Anthropic key, at the same minute

238 tests; claude-haiku-5-5 (anthropic), reasoning low; llx's advisor (Jev) off; 1200 s a run, runs an agent: 1; Darwin arm64 26.6.2, load 11.9.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| llx | llx | f0b338c5 | 2774144 | b4d47a512be0 |
| claude | claude | 2.1.321 (Claude Code) | — | — |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| llx | 0 | yes | 238/238 | 285.8 | 283.3 | 2.5 | 14 | 0 | 0.0500 | 87 | 688603 | 70262 | 14 | 2 | 1 | 0 | 0 |
| claude | 0 | yes | 238/238 | 477.6 | — | — | 25 | — | 0.4604 | 89 | 2208577 | 112299 | 28 | 0 | 0 | 0 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
