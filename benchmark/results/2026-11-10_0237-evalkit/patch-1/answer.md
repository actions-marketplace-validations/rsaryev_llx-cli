All four modules in `src/` are implemented, and `bun test` passes: 238 tests across the four files, 0 failures. Only the `src/` files changed; the tests, `package.json` and `tsconfig.json` are untouched.

- **`src/expr.ts`**: a lexer that reads the whole source before parsing, a recursive-descent parser for the precedence table (including the `**` restrictions), and an evaluator with the builtins and `round` using the decimal digits of `String(x)`.
- **`src/query.ts`**: a SQL lexer and parser, static checks run before any row is read (unknown columns, qualifiers, grouping rules), SQL three-valued logic, `LIKE`, grouping and aggregates, and output, `DISTINCT`, `ORDER BY`, `OFFSET`/`LIMIT`.
- **`src/diff.ts`**: a suffix-cost dynamic programme with a greedy walk that gives the tie-break in D3, unified-diff hunks with the merge rule and no-newline markers, and an exact `applyPatch`.
- **`src/cron.ts`**: field parsing with the macro and day-of-week rules, and `next`, which searches day by day in UTC. It stops after one 146,097-day cycle and throws `schedule never fires`.

Limits: I did not run a type checker, because `bun test` does not type-check and no `tsc` run was requested. No agents were used; I wrote the work myself, in pieces after one attempt to write `expr.ts` in a single call was cut off.
