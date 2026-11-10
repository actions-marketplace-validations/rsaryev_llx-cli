All four modules in `src/` are implemented, and `bun test` passes: 238 of 238 tests across the four files. I changed only `src/expr.ts`, `src/query.ts`, `src/diff.ts` and `src/cron.ts`. The tests, `package.json`, `tsconfig.json` and the exported names, signatures and error classes are untouched.

- **`diff.ts`**: a suffix-table LCS, walked from the start with the keep, then delete, then insert tie-break. Unified diff output, hunk grouping, and exact patch application are built on it.
- **`cron.ts`**: parsing, macros and Vixie day matching in UTC. `next` scans day by day and gives up after one 146,097-day cycle, so an impossible schedule throws `schedule never fires` quickly.
- **`expr.ts`**: the lexer tokenises the whole source before parsing. The parser implements the precedence table, including the `**` restrictions. The evaluator handles the builtins, with `round` working on the exact decimal that `String(x)` writes.
- **`query.ts`**: lexer, parser, column checks before any row is read, three-valued logic, grouping, DISTINCT, ORDER BY and LIMIT/OFFSET in the order the spec gives. Any internal error is converted to a `QueryError`, so callers only ever see that class.

Some choices the spec left open:
- Arrays and objects are not indexable with `.` (arrays are rejected there).
- Non-finite results in `query.ts` arithmetic and `SUM` raise a `QueryError`.
- `env` is copied and normalised on entry, so `undefined` becomes `null` at any depth.

I did not add any tests beyond the fixed ones.
