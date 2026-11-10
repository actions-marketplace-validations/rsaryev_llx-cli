All four modules in `src/` are implemented, and `bun test` passes: 238 of 238 tests.

- **`src/expr.ts`**: lexer, parser and evaluator for the expression language. The lexer runs over the whole source before parsing, so lexer errors win as the spec requires. Error positions, short-circuit operators, lambdas and builtins are implemented as written.
- **`src/query.ts`**: SQL-style query over arrays of objects. Column and qualifier checks run before any row is read. The module covers three-valued logic, grouping and aggregates, DISTINCT, ORDER BY (position, output name, or expression) and OFFSET/LIMIT.
- **`src/diff.ts`**: minimal line diff with the spec's tie-break, from a suffix distance table. It also has unified-diff output with hunk merging and the no-newline marker, and a strict patch applier. The 2,000-line case runs well under a second.
- **`src/cron.ts`**: five-field parser with macros and the day-of-week 7 rules, plus `next`. It walks days in UTC, not minutes, and gives up with "schedule never fires" after one 400-year cycle with no match.

I wrote the code myself; no agents were used. I touched only the four files in `src/`. `test/`, `package.json` and `tsconfig.json` are unchanged, as `git status` confirms.

Two things to know:
- The query module was lost once when a long patch was cut off. I rebuilt it in three smaller patches, and the tests cover the final version.
- Tests run with Bun only. I did not run a TypeScript type check, since `tsconfig.json` is set up for one but the task did not ask for it.
