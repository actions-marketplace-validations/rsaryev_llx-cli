All four modules in `src/` are implemented, and `bun test` passes: 238 tests across the four files, 0 failures.

- **`src/diff.ts`**: a suffix-table LCS for `diffLines`, using the README's tie-break (keep, then delete, then insert). `unifiedDiff` builds hunks with the context-merge rule. `applyPatch` parses the patch and applies it exactly, with no fuzz.
- **`src/cron.ts`**: field parsing with macros and names, and UTC matching using the Vixie day rule. `next` scans day by day and throws `schedule never fires` after a 146,097-day run with no match.
- **`src/expr.ts`**: a lexer that runs over the whole source before parsing, a recursive-descent parser with the README's precedence and the `**` restrictions, and an evaluator with positioned runtime errors and the builtins.
- **`src/query.ts`**: a lexer and parser for the SQL subset, static checks before any row is read, three-valued logic, grouping and aggregates, DISTINCT, ORDER BY resolution, then OFFSET and LIMIT.

I did not touch `test/`, `package.json` or `tsconfig.json`. No dependencies were added. `tsc` reports errors only in the test files, which come from the missing `bun:test` and Node type definitions in this environment; the `src/` files have none.
