# evalkit, 2026-10-31 17:13

Before the cleanup: evalkit's calibration, on a machine busy with builds.

238 tests; gpt-6-luna (openai), reasoning low; llx's advisor (Jev) off; 1500 s a run, runs an agent: 1; Darwin arm64 26.6.2, load ?.

| agent | kind | version | bytes | sha256 |
|---|---|---|---|---|
| base | llx | e554ec72 | 2855744 | e3fd7d9af5b0 |

| agent | run | solved | pass | wall s | model s | the rest s | turns | Jev | cost $ | cache % | input tokens | output tokens | calls | failed commands | patches | failed patches | tampered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 0 | no | 224/238 | 1500.3 | 844.7 | 655.6 | 240 | 1 | 0.4772 | 99 | 40181403 | 64670 | 242 | 15 | 191 | 19 | 0 |

Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, `session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), `src.tar.gz` (the code it wrote) and `answer.md` (its last words).
