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
| [2026-11-02 22:37](results/2026-11-02_2237-shopkit/summary.md) | shopkit | llx | 55514bfe | on, 0 | 1 | 1/1 | 69, 69 of 69 | 30 | 6 | 0.0049 | 54 | 3649 | 1.3 |
| [2026-11-02 22:38](results/2026-11-02_2238-evalkit/summary.md) | evalkit | llx | 55514bfe | on, 1 | 1 | 1/1 | 238, 238 of 238 | 1094.7 | 83 | 0.1108 | 98 | 32397 | 1.7 |
| [2026-11-02 23:55](results/2026-11-02_2355-evalkit/summary.md) | evalkit | llx | bd01149d | on, 2 | 1 | 1/1 | 238, 238 of 238 | 802.8 | 191 | 0.3017 | 99 | 61035 | 2 |
| [2026-11-08 03:45](results/2026-11-08_0345-shopkit/summary.md) | shopkit | llx | 6d592d08-dirty | on, 0 | 1 | 1/1 | 69, 69 of 69 | 45.9 | 3 | 0.1131 | 46 | 7934 | 4.5 |
| [2026-11-08 03:49](results/2026-11-08_0349-shopkit/summary.md) | shopkit | llx | 6d592d08-dirty | on, 0 | 1 | 1/1 | 69, 69 of 69 | 52.5 | 7 | 0.0051 | 80 | 5598 | 2.7 |
| [2026-11-08 03:53](results/2026-11-08_0353-shopkit/summary.md) | shopkit | llx | 6d592d08-dirty | on, 0 | 1 | 1/1 | 69, 69 of 69 | 92.9 | 6 | 0.0045 | 74 | 4761 | 3.7 |
| [2026-11-08 04:00](results/2026-11-08_0400-shopkit/summary.md) | shopkit | llx | b2f8ce14 | on, 0 | 1 | 1/1 | 69, 69 of 69 | 48.2 | 5 | 0.1162 | 66 | 7954 | 6.1 |
| [2026-11-08 04:00](results/2026-11-08_0400-shopkit/summary.md) | shopkit | claude | 2.1.319 (Claude Code) | — | 1 | 1/1 | 69, 69 of 69 | 52.3 | 6 | 0.1149 | 69 | 8380 | 6.1 |
| [2026-11-10 01:55](results/2026-11-10_0155-shopkit/summary.md) | shopkit | llx | 5eef02d7 | off | 1 | 1/1 | 69, 69 of 69 | 65.5 | 4 | 0.0114 | 57 | 16531 | 17.2 |
| [2026-11-10 01:56](results/2026-11-10_0156-evalkit/summary.md) | evalkit | llx | 5eef02d7 | off | 1 | 1/1 | 238, 238 of 238 | 272.3 | 10 | 0.0456 | 83 | 66300 | 21.2 |
| [2026-11-10 02:03](results/2026-11-10_0203-shopkit/summary.md) | shopkit | llx | f0b338c5 | off | 1 | 1/1 | 69, 69 of 69 | 69.1 | 5 | 0.0118 | 71 | 16630 | 14.7 |
| [2026-11-10 02:03](results/2026-11-10_0203-shopkit/summary.md) | shopkit | claude | 2.1.321 (Claude Code) | — | 1 | 1/1 | 69, 69 of 69 | 67.1 | 4 | 0.0110 | 43 | 15963 | 14.7 |
| [2026-11-10 02:04](results/2026-11-10_0204-evalkit/summary.md) | evalkit | llx | f0b338c5 | off | 1 | 1/1 | 238, 238 of 238 | 285.8 | 14 | 0.0500 | 87 | 70262 | 11.9 |
| [2026-11-10 02:04](results/2026-11-10_0204-evalkit/summary.md) | evalkit | claude | 2.1.321 (Claude Code) | — | 1 | 1/1 | 238, 238 of 238 | 477.6 | 25 | 0.4604 | 89 | 112299 | 11.9 |
| [2026-11-10 02:36](results/2026-11-10_0236-shopkit/summary.md) | shopkit | base | llx 0.5.0 | off | 2 | 2/2 | 69, 69 of 69 | 55.65 | 5 | 0.0107 | 67 | 14239.5 | 10.4 |
| [2026-11-10 02:36](results/2026-11-10_0236-shopkit/summary.md) | shopkit | patch | llx 0.5.0 | off | 2 | 2/2 | 69, 69 of 69 | 79.65 | 6 | 0.0160 | 74 | 22736 | 10.4 |
| [2026-11-10 02:37](results/2026-11-10_0237-evalkit/summary.md) | evalkit | base | llx 0.5.0 | off | 2 | 2/2 | 238, 238 of 238 | 314.95 | 14.5 | 0.0822 | 87.5 | 77378.5 | 9.3 |
| [2026-11-10 02:37](results/2026-11-10_0237-evalkit/summary.md) | evalkit | patch | llx 0.5.0 | off | 2 | 2/2 | 238, 238 of 238 | 545.95 | 17.5 | 0.2582 | 91 | 133182.5 | 9.3 |
| [2026-11-10 02:52](results/2026-11-10_0252-shopkit/summary.md) | shopkit | base | llx 0.5.0 | off | 1 | 1/1 | 69, 69 of 69 | 56.4 | 6 | 0.0110 | 70 | 14490 | 3.2 |
| [2026-11-10 02:52](results/2026-11-10_0252-shopkit/summary.md) | shopkit | fix | llx 0.5.0 | off | 1 | 1/1 | 69, 69 of 69 | 69.1 | 6 | 0.0127 | 69 | 16923 | 3.2 |
| [2026-11-10 02:52](results/2026-11-10_0252-shopkit/summary.md) | shopkit | fixrule | llx 0.5.0 | off | 1 | 1/1 | 69, 69 of 69 | 66.5 | 4 | 0.0121 | 49 | 17035 | 3.2 |
| [2026-11-10 02:53](results/2026-11-10_0253-evalkit/summary.md) | evalkit | base | llx 0.5.0 | off | 2 | 2/2 | 238, 238 of 238 | 399.9 | 13.5 | 0.1553 | 85.5 | 97626.5 | 5.9 |
| [2026-11-10 02:53](results/2026-11-10_0253-evalkit/summary.md) | evalkit | fix | llx 0.5.0 | off | 2 | 2/2 | 238, 238 of 238 | 249.1 | 9.5 | 0.0425 | 80.5 | 61336 | 5.9 |
| [2026-11-10 02:53](results/2026-11-10_0253-evalkit/summary.md) | evalkit | fixrule | llx 0.5.0 | off | 2 | 2/2 | 238, 238 of 238 | 397 | 14 | 0.1553 | 83 | 98014 | 5.9 |
| [2026-11-10 03:08](results/2026-11-10_0308-shopkit/summary.md) | shopkit | base | llx 0.5.0 | off | 1 | 1/1 | 69, 69 of 69 | 62.4 | 4 | 0.0112 | 50 | 15654 | 6.1 |
| [2026-11-10 03:08](results/2026-11-10_0308-shopkit/summary.md) | shopkit | new | llx 0.5.0 | off | 1 | 1/1 | 69, 69 of 69 | 67.3 | 6 | 0.0125 | 74 | 16475 | 6.1 |
| [2026-11-10 03:09](results/2026-11-10_0309-evalkit/summary.md) | evalkit | base | llx 0.5.0 | off | 3 | 3/3 | 238, 238 of 238 | 477.9 | 19 | 0.1716 | 93 | 116139 | 3.7 |
| [2026-11-10 03:09](results/2026-11-10_0309-evalkit/summary.md) | evalkit | new | llx 0.5.0 | off | 3 | 3/3 | 238, 238 of 238 | 268.8 | 13 | 0.0477 | 88 | 66748 | 3.7 |
| [2026-11-10 03:28](results/2026-11-10_0328-shopkit/summary.md) | shopkit | llx | 3ca3ebc2 | off | 2 | 2/2 | 69, 69 of 69 | 45.25 | 4 | 0.1045 | 70.5 | 7858.5 | 7.8 |
| [2026-11-10 03:28](results/2026-11-10_0328-shopkit/summary.md) | shopkit | claude | 2.1.321 (Claude Code) | — | 2 | 2/2 | 69, 69 of 69 | 78.55 | 9.5 | 0.1751 | 76.5 | 13207.5 | 7.8 |
