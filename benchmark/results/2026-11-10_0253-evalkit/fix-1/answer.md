All four modules in `src/` are implemented, and `bun test` passes: 238 of 238 tests across the four files. I changed only `src/diff.ts`, `src/cron.ts`, `src/expr.ts` and `src/query.ts`; the tests, `package.json` and `tsconfig.json` are untouched.

- **`diff.ts`**: an exact suffix-distance table gives the minimal edit script, and a greedy walk applies the keep-then-delete-then-insert tie-break. Unified diff output and patch application follow D4–D13, including the no-newline marker and exact-match checks.
- **`cron.ts`**: parsing covers fields, names, steps, macros and the day-of-week 7 rules. `next` walks days in UTC and gives up with `schedule never fires` after one 400-year cycle without a match.
- **`expr.ts`**: the lexer runs over the whole source before parsing, so lexer errors win. The parser is precedence-climbing, with the `**` restriction on prefix operands. The evaluator uses lexical scopes for lambdas, and `round` uses exact decimal arithmetic with ties to even.
- **`query.ts`**: a parser builds an AST. Every column reference and grouping rule is checked before any row is read. Then the query filters, groups, computes the select list, applies DISTINCT, sorts stably, and applies OFFSET and LIMIT. Non-`QueryError` exceptions are converted to `QueryError`.

I also spot-checked a few spec examples the tests don't cover, such as error positions, `round(2.675, 2)`, alias resolution in ORDER BY, and a never-firing cron schedule. They all matched the spec.

I did not run a TypeScript type check, since the test command doesn't use one. I wrote the code to `strict`, but it has not been checked with `tsc`.
