# Search benchmark history

A line per recorded run (`bun benchmark/grep/run.ts --record`): the label, then R@5, P@5 and MRR
over all the questions, for each mode of `llx grep`.

- 2026-11-02 v1 (the words and the decision model, first): judged R@5 90% P@5 79% MRR 0.90; words R@5 68% P@5 39% MRR 0.64
- 2026-11-02 v2 (numbers in any spelling, tests out of the judged files, `data/` at half): judged R@5 90% P@5 79% MRR 0.85; words R@5 80% P@5 43% MRR 0.68
- 2026-11-02 v4 (parts chosen whole and apart, not chained): judged R@5 97% P@5 77% MRR 0.89; words R@5 80% P@5 43% MRR 0.68
- 2026-11-02 v5 (shown within 0.25 of the best, the rest named as unsure): judged R@5 97% P@5 81% MRR 0.86; words R@5 80% P@5 43% MRR 0.68
