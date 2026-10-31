Implemented all six modules in `src/`: money, cart, CSV, router, LRU cache, and semver.

**Verification:** `bun test` passes all 69 tests (284 assertions). `bun build src/*.ts` also succeeds. `git diff --check` is clean. A separate `bunx tsc --noEmit` check could not complete because the project lacks `bun:test` type declarations.
