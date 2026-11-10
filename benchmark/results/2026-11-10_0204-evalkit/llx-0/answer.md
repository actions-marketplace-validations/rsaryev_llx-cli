All four modules in `src/` are implemented, and `bun test` passes all 238 tests. The tests, `package.json` and `tsconfig.json` are unchanged, and the only changed files are the four `src/` modules.

- **`src/diff.ts`**: a suffix DP gives the minimal edit script, and a greedy walk applies the keep, then delete, then insert tie-break. Unified diffs use the hunk and context rules, and `applyPatch` checks each hunk exactly.
- **`src/cron.ts`**: field parsing with the day-of-week 7 rules and the macros. `next` walks day by day in UTC and gives up after one 400-year Gregorian cycle, so an impossible schedule throws `schedule never fires` promptly. One fix was needed during testing: single values were not checked against the field minimum, so `0` was accepted for day-of-month and month.
- **`src/expr.ts`**: lexer over the whole source first, then a recursive-descent parser. Prefix operators bind looser than `**`, so `-2 ** 2` is a syntax error at the `**`. Lambdas are detected by lookahead. The evaluator covers the operators, the builtins and the rounding rule, which works on the exact decimal text of the number (so `round(2.675, 2)` is 2.68).
- **`src/query.ts`**: lexer and parser for the SQL subset. Column, qualifier, aggregate and GROUP BY checks run before any row is read. Evaluation uses three-valued logic, grouping, DISTINCT, a stable ORDER BY with the type checks, then OFFSET and LIMIT. Any unexpected internal error is turned into a `QueryError`.

I also ran the suite under `TZ=America/New_York` and `TZ=Asia/Kolkata` and it still passed. Running `tsc --strict` on `src/*.ts` reports no errors. Running it on the whole project also flags the test files, but only because `bun:test` types are not installed here.

Limits: the spec does not say whether a WHERE condition that is not boolean is an error when the table is empty. I raise it only when a row is evaluated, so an empty table does not raise it. The tests do not cover that case.
