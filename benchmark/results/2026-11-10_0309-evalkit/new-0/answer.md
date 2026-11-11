All four modules in `src/` are implemented as the README specifies, and `bun test` passes: 238 of 238 tests, across four time zones.

- **`src/diff.ts`**: a suffix dynamic programme gives the minimal edit script, and a greedy walk applies the keep-then-delete-then-insert tie-break. It also covers unified diff output and strict patch application.
- **`src/cron.ts`**: parsing, macros and UTC matching. `next` searches day by day and reports "schedule never fires" after one 400-year Gregorian cycle with no match.
- **`src/expr.ts`**: lexer, parser and evaluator. The lexer runs over the whole source before parsing, so lexer errors win, as the spec requires. `**` after a prefix operator is rejected at the `**`.
- **`src/query.ts`**: tokenizer, parser, static reference and grouping checks, then the pipeline in the order Q28 gives: WHERE, grouping, HAVING, select list, DISTINCT, ORDER BY, OFFSET, LIMIT. Any internal failure is re-thrown as a `QueryError`.

Two things I decided where the spec was open:
- A negative zero from arithmetic is returned as `0`.
- In `query`, an error that is not a `QueryError` is converted into one, so every rejected query throws `QueryError`.

I also fixed a few TypeScript type errors in `src/` that the tests did not catch. `tsc` now reports no errors in `src/`. The test files still show `bun:test` type errors under plain `tsc`, which is an existing setup issue and outside this task.

I changed only the four files in `src/`. I did not touch the tests, `package.json` or `tsconfig.json`, and I did not commit anything. I did not use any sub-agents.
