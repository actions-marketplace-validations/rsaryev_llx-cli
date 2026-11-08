I implemented all six modules in `src/` as the README specifies, and `bun test` now passes all 69 tests. I didn't change the tests, `package.json` or `tsconfig.json`.

My first run had one failure, in the semver prerelease ordering (a prerelease must sort below the same version without one). I fixed that comparison and reran.

The README leaves a few cases open, so I made these choices:
- **Cart tax:** it uses floating-point arithmetic with a tiny epsilon to round half up. That could be off by a cent for unusual fractional tax rates.
- **`OFF<n>` codes:** the subtotal check happens when the code is applied. If the cart changes afterwards, `discount()` is still capped at the subtotal, but the code isn't re-validated.
- **LRU:** expired entries are purged before eviction, so a live entry is never evicted to make room while an expired one is still sitting in the cache.
- **Router:** two patterns that differ only in param names (`/users/:id` and `/users/:name`) count as the same pattern, so adding both throws `RouterError`.
- **Semver ranges:** I handled partial versions like `>=1.2` and `^1.2` by filling missing parts with zero for the lower bound. The tests don't cover this and the README doesn't spell it out.
