# evalkit, 2026-11-02 23:55

B9 with the fixes from localix: advice reads arrivals, no monitor for a moved job, the history shows its end (bd01149d)

238 tests; gpt-6-luna (openai), reasoning low; llx's advisor (Jev) on; 1200 s a run, runs an agent: 1; Darwin arm64 26.6.2, load 2.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| llx | llx | bd01149d | 2407792 | 136a21d8e49b |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| llx | 0 | yes | 238/238 | 802.8 | 781 | 21.8 | 191 | 2 | 0.3017 | 99 | 25064734 | 61035 | 190 | 15 | 104 | 1 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
