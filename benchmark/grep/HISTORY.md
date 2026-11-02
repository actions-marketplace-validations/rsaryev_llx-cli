# Search benchmark history

A line per recorded run (`bun benchmark/grep/run.ts --record`): the label, then R@5, P@5 and MRR
over all the questions, for each mode of `llx grep`.

- 2026-11-02 v1 (the words and the decision model, first): judged R@5 90% P@5 79% MRR 0.90; words R@5 68% P@5 39% MRR 0.64
- 2026-11-02 v2 (numbers in any spelling, tests out of the judged files, `data/` at half): judged R@5 90% P@5 79% MRR 0.85; words R@5 80% P@5 43% MRR 0.68
- 2026-11-02 v4 (parts chosen whole and apart, not chained): judged R@5 97% P@5 77% MRR 0.89; words R@5 80% P@5 43% MRR 0.68
- 2026-11-02 v5 (shown within 0.25 of the best, the rest named as unsure): judged R@5 97% P@5 81% MRR 0.86; words R@5 80% P@5 43% MRR 0.68
- 2026-11-02 v6 (the model also names a long file's best part, shown first): judged R@5 97% P@5 81% MRR 0.91, $0.0048 a question
- 2026-11-02 v10 (a second round through the modules the kept files name: those not read and those the model doubted, read where they name them, told how they were reached, shown after the first round's; none for a definition): judged R@5 98% P@5 80% MRR 0.89, $0.0053 a question, 3.9 s
- 2026-11-02 v12 (24 files judged at once; the second round reads again what the first left below its margin, each round held to its own best): judged R@5 98% R@10 100% P@5 79% MRR 0.87, 3.1 s; again: R@5 98% R@10 100% P@5 80% MRR 0.89, 2.1 s; $0.0053 a question
