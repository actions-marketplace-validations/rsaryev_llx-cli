I implemented all six modules in `src/` as the README specifies, and `bun test` passes (69 tests, 0 failures). I did not touch the tests, `package.json` or `tsconfig.json`.

The README left a few points open, and I chose as follows:
- **Router duplicates:** two patterns count as the same if they have the same shape, ignoring param names. So `/a/:id` and `/a/:name` conflict.
- **LRU expiry:** an entry counts as expired once `ttlMs` or more has passed since it was set.
- **LRU eviction:** when the cache is over capacity, expired entries are dropped first without calling `onEvict`, then the least recent live entry is evicted.
- **Cart `OFF` codes:** the subtotal check happens when the code is applied. If the cart shrinks afterwards, `discount()` is still capped at the subtotal.

I wrote the semver range handling (carets, tildes, x-ranges, the prerelease rule) from the README alone. The tests pass, but I did not test unusual ranges beyond what they cover.
