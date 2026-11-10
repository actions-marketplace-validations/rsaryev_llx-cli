All four modules in `src/` are implemented, and `bun test` passes: 238 tests across the four files, 0 failures.

- **`src/expr.ts`**: lexer, parser and evaluator for the expression language.
- **`src/query.ts`**: SQL-style `query` over arrays of objects.
- **`src/diff.ts`**: line diff with the required tie-break, unified diff, and strict patch application.
- **`src/cron.ts`**: five-field UTC cron parsing and `next`.

I didn't change any test, `package.json` or `tsconfig.json`, and the exported names, signatures and error classes are unchanged. There are no dependencies.

I fixed two bugs along the way. In cron, the "7 means Sunday" mapping was applied to every field, so July was dropped from the month set. In expr, the precedence recursion had the wrong bound. Both are fixed and the tests pass.

Running `tsc --noEmit` reports no errors in `src/`. It does report errors in the test files, because Bun's type declarations aren't installed in this environment. I left those alone since the tests are fixed.
