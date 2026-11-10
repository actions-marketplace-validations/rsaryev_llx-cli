All four modules in `src/` are implemented, and `bun test` passes: 238 tests, 0 failures. Only the four `src/` files changed. The tests, `package.json` and `tsconfig.json` are untouched.

- **`src/diff.ts`** (47 tests): the edit script comes from a suffix dynamic programme, with the README's keep-before-delete-before-insert tie-break. Unified diffs and patch application follow the README's hunk, newline-marker and exact-match rules.
- **`src/cron.ts`** (46 tests): parsing with the field, step, name and day-of-week 7 rules. `next` works in UTC by day, applies the Vixie day rule, and gives up after one 400-year cycle (146,097 days) with no match.
- **`src/expr.ts`** (72 tests): the whole source is lexed before parsing, so lexer errors win. A recursive-descent parser follows the precedence table, including the `**` restrictions. The evaluator handles scopes, short-circuit operators, builtins, and `round` with exact decimal rounding and ties to even.
- **`src/query.ts`** (73 tests): a lexer and parser for the clause order and expression grammar. Static checks (columns, qualifiers, aggregate placement, GROUP BY) run before any row is read. Execution covers three-valued logic, grouping, aggregates, DISTINCT, ORDER BY kinds, and OFFSET/LIMIT.

One bug came up along the way. A grouped query with no `GROUP BY` put its rows into two buckets, which broke five query tests. It is fixed, and the full suite passes after the fix.

Nothing is left open. I did not spawn any agents; all the work was done directly.
