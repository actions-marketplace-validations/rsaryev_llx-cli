All four modules in `src/` are implemented, and `bun test` passes: 238 tests across the four files, with no failures.

- **`src/diff.ts`**: a suffix-distance dynamic programme gives the minimal edit script, with the tie-break from D3. The unified diff writer and the patch applier follow D5–D12.
- **`src/cron.ts`**: field parsing with the C2–C7 rules, macros, and a day-by-day scan in UTC for `next`. The scan gives up after one 146,097-day cycle with no match, so impossible schedules throw promptly.
- **`src/expr.ts`**: lexer, recursive-descent parser and evaluator. Lexing finishes before parsing starts, so lexer errors win. The `**` rule (E10) is handled by giving a prefix operator's operand a postfix-only grammar. `round` uses exact decimal arithmetic on `String(x)` with ties to even.
- **`src/query.ts`**: lexer, parser, static checks (column names, qualifiers, GROUP BY rules, aggregate placement) that run before any row is read, then three-valued logic, aggregates, DISTINCT, a stable ORDER BY with the per-key type check, and OFFSET/LIMIT.

I changed only files under `src/`. The tests, `package.json` and `tsconfig.json` are untouched. I also ran `tsc` over `src/` and it reports no errors. The type errors `tsc` reports for the test files come from `bun:test` and `process` types not being installed here, not from the code.

A few things are worth knowing:
- The expression evaluator keeps the current environment in a module-level variable while it runs. That is fine for synchronous calls, but two evaluations cannot overlap.
- I did not commit anything.
