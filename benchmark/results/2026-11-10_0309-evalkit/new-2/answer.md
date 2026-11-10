All four modules in `src/` are implemented, and `bun test` passes: 238 of 238 tests across the four files.

- **`src/diff.ts`**: a suffix DP gives the minimal edit script, with a greedy walk that applies the keep, delete, insert tie-break. Unified diff output and strict patch application are built on it. A 2,000-line diff takes about 13 ms, and applying it returns the target text.
- **`src/cron.ts`**: parsing and matching are in UTC, with the Vixie day rule. `next` scans UTC days rather than minutes. It throws `schedule never fires` after the 146,097-day cycle, and that check takes about 3 ms.
- **`src/expr.ts`**: the lexer reads the whole source before parsing, so lexer errors win over parser errors. The parser follows the precedence table, and `**` rejects prefix-operator left operands. The evaluator covers the operators, the builtins, and the `round` rule that uses the exact decimal of `String(x)`.
- **`src/query.ts`**: the parser records each select item's source text for its output key. Static checks cover column names, qualifiers, aggregates, GROUP BY rules, and ORDER BY kinds, all before any row is read. Execution then runs WHERE, grouping, HAVING, the select list, DISTINCT, a stable ORDER BY, and OFFSET/LIMIT. Any unexpected internal error is re-thrown as a `QueryError`.

Only files under `src/` changed. I did not touch the tests, `package.json` or `tsconfig.json`. I did not use agents, and I did not commit anything.

Three choices the spec leaves open:
- A query whose arithmetic overflows to infinity raises a `QueryError`.
- A non-boolean WHERE condition is checked per row, so an empty table never raises it.
- `expr` keeps the current environment in a module-level variable, which is safe because evaluation is synchronous.
