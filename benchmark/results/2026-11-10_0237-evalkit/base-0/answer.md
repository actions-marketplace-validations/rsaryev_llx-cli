All four modules in `src/` are implemented as the README specifies, and `bun test` passes all 238 tests. The tests also pass under four different `TZ` settings. The test files, `package.json` and `tsconfig.json` are unchanged; only the four files in `src/` were modified.

- **`src/diff.ts`**: a suffix-table LCS for the minimal edit script, with the keep/delete/insert tie-break. Unified diff output and patch application follow the hunk rules. Patches are applied exactly, with no fuzz.
- **`src/cron.ts`**: parsing with leftmost-field errors and macros. Matching uses the Vixie day rule and runs in UTC. `next` walks days for at most 146,097 days, so a schedule that never fires throws quickly.
- **`src/expr.ts`**: lexer, precedence-climbing parser with the `**`/prefix rules, and an evaluator with positioned errors. Builtins include exact decimal `round` with ties-to-even.
- **`src/query.ts`**: lexer and parser, static column checks before any row is read, three-valued logic, grouping and aggregates, DISTINCT, ORDER BY resolution (position, output name, or expression), and OFFSET/LIMIT.

Some choices the spec left open:
- In `expr`, member access on an array (`arr.x`) is an error, since arrays are not "objects" in the spec's sense.
- In `cron`, `next` throws "schedule never fires" if it cannot find `count` matches within the 400-year window, even if some matches were found.

I did not use any sub-agents; the work was small enough to do in sequence.
