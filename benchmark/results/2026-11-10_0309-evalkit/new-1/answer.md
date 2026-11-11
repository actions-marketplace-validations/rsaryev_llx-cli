All four modules in `src/` are implemented, and `bun test` passes: 238 tests across 4 files, 0 failures. I left the tests, `package.json` and `tsconfig.json` untouched, and only the four `src/` files changed.

- **`src/expr.ts`**: a lexer that runs over the whole source before parsing, a recursive-descent parser that follows the precedence table (including the `**` and prefix rules), and an evaluator with the builtins. Environment values are copied once up front, with `undefined` turned into `null`.
- **`src/query.ts`**: a SQL lexer and parser for the clause order in Q4. A static pass checks column references, qualifiers and grouping before any row is read. Rows are then filtered, grouped, projected, de-duplicated, sorted and paged, with three-valued logic throughout.
- **`src/diff.ts`**: a suffix-DP edit script with the README's keep-then-delete-then-insert tie-break, unified-diff output with hunk merging and no-newline markers, and an exact patch applier. A 2,000-line diff runs in about 16 ms and round-trips.
- **`src/cron.ts`**: field parsing with the name, step, 7-as-Sunday and malformed-field rules, plus a day-by-day search in UTC. Schedules that never fire are reported after one 400-year cycle, which takes about 1 ms.

I also spot-checked README examples beyond the test suite: error positions such as `) #` at 1:3, `1 < 2 < 3` at the second `<`, banker's rounding of `2.675`, `(a, b) => a` in `map`, and the `0 0 29 2 */7` schedule. They all match the spec.

I did not run a TypeScript type check, since `tsc` is not installed here and `bun test` does not type-check. The code is written to the strict config, but that is unverified.
