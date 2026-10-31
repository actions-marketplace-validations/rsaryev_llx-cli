# Benchmark history

A row per agent per run of `run.ts`, its medians over the runs: whether it solved the task (every test passed), the time it took, the turns (model requests), llx's advisor (Jev): on or off, and how often it sent the model back, the cost at the model's price, the share of input read from the prompt cache. Made again from `results/*/run.json` by each run (`bun benchmark/run.ts --history`).

| when | task | agent | version | Jev | runs | solved | pass (median, min) | wall s | turns | cost $ | cache % | output tokens | load |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| [2026-10-31 16:15](results/2026-10-31_1615-shopkit/summary.md) | shopkit | base | e554ec72 | off | 1 | 1/1 | 69, 69 of 69 | 39.7 | 5 | 0.0043 | 68 | 4778 | ? |
| [2026-10-31 17:13](results/2026-10-31_1713-evalkit/summary.md) | evalkit | base | e554ec72 | off | 1 | 0/1 | 224, 224 of 238 | 1500.3 | 240 | 0.4772 | 99 | 64670 | ? |
| [2026-10-31 20:46](results/2026-10-31_2046-shopkit/summary.md) | shopkit | base | e554ec72 | off | 3 | 3/3 | 69, 69 of 69 | 44.5 | 9 | 0.0052 | 84 | 4983 | 5.5 |
| [2026-10-31 20:46](results/2026-10-31_2046-shopkit/summary.md) | shopkit | new | eb9993c4 | off | 3 | 3/3 | 69, 69 of 69 | 42.3 | 8 | 0.0055 | 81 | 5383 | 5.5 |
| [2026-10-31 20:47](results/2026-10-31_2047-evalkit/summary.md) | evalkit | base | e554ec72 | off | 2 | 0/2 | 186, 160 of 238 | 221.6 | 40 | 0.0504 | 93 | 18167 | 5.5 |
| [2026-10-31 20:47](results/2026-10-31_2047-evalkit/summary.md) | evalkit | new | eb9993c4 | off | 2 | 0/2 | 183.5, 150 of 238 | 262.15 | 47.5 | 0.0622 | 90.5 | 22406 | 5.5 |
| [2026-10-31 21:40](results/2026-10-31_2140-shopkit/summary.md) | shopkit | llx | 2a7351f7 | off | 1 | 1/1 | 69, 69 of 69 | 41.1 | 8 | 0.0051 | 82 | 5374 | 5.7 |
| [2026-10-31 21:48](results/2026-10-31_2148-evalkit/summary.md) | evalkit | llx | 60c718f3 | on, 2 | 1 | 1/1 | 238, 238 of 238 | 594.9 | 158 | 0.2843 | 98 | 52086 | 2.5 |
| [2026-10-31 22:02](results/2026-10-31_2202-shopkit/summary.md) | shopkit | llx | f9833d52 | on, 0 | 1 | 1/1 | 69, 69 of 69 | 43.1 | 8 | 0.0048 | 82 | 4731 | 1.6 |
| [2026-10-31 22:12](results/2026-10-31_2212-shopkit/summary.md) | shopkit | codex | codex-cli 0.163.0 | — | 1 | 1/1 | 69, 69 of 69 | 50.7 | 17 | 0.0088 | 95 | 5864 | 12.2 |
| [2026-10-31 22:13](results/2026-10-31_2213-evalkit/summary.md) | evalkit | codex | codex-cli 0.163.0 | — | 1 | 0/1 | 197, 197 of 238 | 148.5 | 23 | 0.0248 | 89 | 14227 | 6 |
