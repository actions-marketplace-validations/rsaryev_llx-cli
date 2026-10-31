# shopkit, 2026-10-31 20:46

A/B: before the cleanup (e554ec72) and after it and the crate split (eb9993c4).

69 tests; gpt-6-luna (openai), reasoning low; llx's advisor (Jev) off; 600 s a run, runs an agent: 3; Darwin arm64 26.6.2, load 5.5.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| base | llx | e554ec72 | 2855744 | e3fd7d9af5b0 |
| new | llx | eb9993c4 | 2856016 | facddfc97ecb |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 0 | yes | 69/69 | 47.6 | 46.3 | 1.3 | 15 | 0 | 0.0066 | 89 | 195808 | 5195 | 17 | 1 | 9 | 0 | 0 |
| new | 0 | yes | 69/69 | 42.3 | 41.8 | 0.5 | 9 | 0 | 0.0061 | 81 | 116662 | 5868 | 11 | 3 | 3 | 0 | 0 |
| base | 1 | yes | 69/69 | 44.5 | 44 | 0.5 | 8 | 0 | 0.0052 | 77 | 90140 | 4983 | 9 | 1 | 2 | 0 | 0 |
| new | 1 | yes | 69/69 | 42.5 | 42.1 | 0.4 | 8 | 0 | 0.0055 | 78 | 94552 | 5383 | 11 | 2 | 3 | 0 | 0 |
| base | 2 | yes | 69/69 | 41.9 | 41 | 0.9 | 9 | 0 | 0.0051 | 84 | 108997 | 4917 | 9 | 2 | 4 | 0 | 0 |
| new | 2 | yes | 69/69 | 34.6 | 34.5 | 0.1 | 8 | 0 | 0.0043 | 82 | 85805 | 4104 | 9 | 0 | 3 | 0 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
