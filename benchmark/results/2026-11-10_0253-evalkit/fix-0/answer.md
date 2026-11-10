All four modules in `src/` are implemented, and `bun test` passes: 238 of 238 tests across the four files.

- **`src/diff.ts`**: an exact suffix-based diff with the README's tie-break, unified diff output with hunk merging and the no-newline marker, and a strict patch applier.
- **`src/cron.ts`**: five-field parsing with macros and the day-of-week 7 rules. `next` walks days, then the sorted hour and minute sets, in UTC. A schedule with no match in one 146,097-day cycle throws `schedule never fires`.
- **`src/expr.ts`**: a lexer that tokenizes the whole source before parsing, a parser with the README's precedence (including the `**` restrictions), and an evaluator with lexical lambda scopes and the builtins. `round` uses the exact decimal digits of `String(x)` with ties to even.
- **`src/query.ts`**: a SQL-style lexer and parser, a static pass that resolves every column and grouping reference before any row is read, then three-valued evaluation, grouping, `DISTINCT`, stable `ORDER BY`, and `OFFSET`/`LIMIT`.

I changed only the four files in `src/`. The tests, `package.json` and `tsconfig.json` are untouched, and I made no commits. I did not use sub-agents. The passing suite is the only check: I did not write extra tests against the README beyond it.
