All four modules in `src/` are implemented, and `bun test` passes: 238 tests across the four test files, 0 failures. I changed nothing in `test/`, `package.json` or `tsconfig.json`, and I did not commit anything.

- **`src/diff.ts`**: `diffLines` builds a suffix edit-distance table and walks it from the start, preferring keep, then delete, then insert. That gives the tie-break the spec asks for. `unifiedDiff` groups changes into hunks with the merge rule and writes the no-newline markers. `applyPatch` parses the patch and applies it exactly, rejecting any mismatch.
- **`src/cron.ts`**: `parseCron` expands each field, handles names, steps, the day-of-week 7 rules and the macros. `next` walks forward day by day in UTC, with the Vixie day rule. It gives up with `schedule never fires` once a gap exceeds 146,097 days.
- **`src/expr.ts`**: a lexer that reads the whole source before parsing, a recursive-descent parser with the precedence rules, and an evaluator with the builtins. `round` works on the exact decimal that `String(x)` writes, with ties to even.
- **`src/query.ts`**: a SQL lexer and parser, then an evaluator with three-valued logic, grouping, aggregates, DISTINCT, ORDER BY and paging. The static checks (unknown columns, grouping rules, aggregate placement) run before any row is read.

Two things to know:
- The expression evaluator keeps the normalised environment in a module-level variable. This is safe because evaluation is synchronous, but it is a shortcut; passing it explicitly would be cleaner.
- The only fix after the first full run was in `query.ts`: WHERE clauses were being checked against the GROUP BY rule. WHERE is now exempt from that check.
