All four modules in `src/` are implemented, and `bun test` passes: 238 of 238 tests across the four files. I changed only `src/`; the tests, `package.json` and `tsconfig.json` are untouched.

- **`src/cron.ts`**: field parsing with the names, steps, the day-of-week 7 rule and the macros. `next` walks the calendar one day at a time, not minute by minute, and throws `schedule never fires` after a full 400-year cycle with no match.
- **`src/diff.ts`**: a suffix-table minimal diff with the keep, delete, insert tie-break; unified-diff output with hunk merging; and a strict patch applier that checks every hunk against the file.
- **`src/expr.ts`**: lexer, recursive-descent parser for the precedence table (including the `**` prefix rule), and an evaluator with lexical scopes, the eight builtins, and banker's rounding done on the exact decimal form of the number.
- **`src/query.ts`**: lexer and parser, static checks before any row is read (column references, qualifiers, aggregate rules), three-valued logic, grouping, output keys, `ORDER BY` resolution, `DISTINCT`, then `OFFSET` and `LIMIT`. Any internal failure is rethrown as a `QueryError`.

I also ran `tsc --noEmit`. The `src/` files have no type errors. The remaining errors are in the test files, because the project has no bun type definitions; I left them alone since the tests are fixed.

I did not use any sub-agents, commit anything, or touch git.
