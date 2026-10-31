The task is not complete: `bun test` still fails (150 passed, 88 failed).

I changed only files under `src/`, as requested. `src/query.ts` remains an unimplemented stub. The expression implementation is incomplete, including lambda parsing and evaluation. The diff implementation has failing patch round-trip and validation cases. Cron parsing and searching were implemented, but the suite did not pass.

I can’t claim the acceptance criteria are met; all tests need to pass before this is done.
