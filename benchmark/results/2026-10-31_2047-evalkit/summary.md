# evalkit, 2026-10-31 20:47

A/B: before the cleanup (e554ec72) and after it and the crate split (eb9993c4).

238 tests; gpt-6-luna (openai), reasoning low; llx's advisor (Jev) off; 1200 s a run, runs an agent: 2; Darwin arm64 26.6.2, load 5.5.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| base | llx | e554ec72 | 2855744 | e3fd7d9af5b0 |
| new | llx | eb9993c4 | 2856016 | facddfc97ecb |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 0 | no | 212/238 | 130.2 | 105.2 | 25 | 14 | 0 | 0.0228 | 88 | 729281 | 14755 | 18 | 4 | 6 | 0 | 0 |
| new | 0 | no | 217/238 | 442.5 | 420 | 22.5 | 85 | 0 | 0.1116 | 98 | 7877376 | 38204 | 86 | 3 | 61 | 2 | 0 |
| base | 1 | no | 160/238 | 313 | 272.2 | 40.8 | 66 | 0 | 0.0779 | 98 | 5651335 | 21579 | 67 | 5 | 9 | 1 | 0 |
| new | 1 | no | 150/238 | 81.8 | 56.3 | 25.5 | 10 | 0 | 0.0127 | 83 | 372367 | 6608 | 11 | 2 | 5 | 0 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
