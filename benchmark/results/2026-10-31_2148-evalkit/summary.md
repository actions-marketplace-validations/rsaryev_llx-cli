# evalkit, 2026-10-31 21:48

Jev on by default from now: the first evalkit run with the advisor.

238 tests; gpt-6-luna (openai), reasoning low; llx's advisor (Jev) on; 1200 s a run, runs an agent: 1; Darwin arm64 26.6.2, load 2.5.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| llx | llx | 60c718f3 | 2856016 | ad15687e29a6 |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| llx | 0 | yes | 238/238 | 594.9 | 569.6 | 25.3 | 158 | 2 | 0.2843 | 98 | 22523958 | 52086 | 157 | 28 | 73 | 6 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
