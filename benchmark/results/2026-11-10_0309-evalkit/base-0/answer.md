All four modules in `src/` are implemented, and `bun test` passes: 238 tests across the four files, 0 failing.

- **`src/expr.ts`**: the lexer reads the whole source before parsing, so a lexer error wins over an earlier parser error. The parser follows the precedence table, including the `**` and prefix rules. The evaluator covers the operators, lambdas, the builtins and `round`, which uses the exact decimal digits of `String(x)` with half-to-even rounding.
- **`src/query.ts`**: a lexer and parser for the clause order, then a static pass. That pass checks column names, qualifiers, grouping rules, aggregate placement and ORDER BY kinds before any row is read. Evaluation then runs WHERE, grouping, HAVING, the select list, DISTINCT, ORDER BY, OFFSET and LIMIT in that order.
- **`src/diff.ts`**: the edit script comes from a suffix dynamic programme, walked from the start with keep, then delete, then insert. Unified diff output and patch application follow the spec, including the no-newline markers and exact hunk matching.
- **`src/cron.ts`**: field parsing covers names, steps, the day-of-week 7 rule and macros. `next` walks UTC days and gives up after one 146,097-day cycle, so "schedule never fires" comes back quickly.

I did not touch `test/`, `package.json` or `tsconfig.json`; `git status` shows only the four `src/` files modified. There are no dependencies. My first write of `query.ts` was cut off by the output limit, so I rewrote it in two parts. I did not use any agents.
