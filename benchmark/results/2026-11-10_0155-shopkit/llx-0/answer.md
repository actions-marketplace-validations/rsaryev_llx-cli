I implemented all six modules in `src/` as the README specifies, and `bun test` now passes: 69 tests across 6 files, 0 failures. I did not change anything in `test/`, `package.json` or `tsconfig.json`, and I did not commit.

The modules are `money.ts`, `cart.ts`, `csv.ts`, `router.ts`, `lru.ts` and `semver.ts`. Beyond what the tests cover, I made a few choices the README leaves open:

- **Cart:** `remove` with a quantity at or above the line's quantity removes the whole line. A fixed-amount code is checked against the subtotal when applied, and the discount is then capped at the subtotal later.
- **Semver:** numeric prerelease identifiers with a leading zero (such as `rc.01`) are rejected, following the semver spec.
- **LRU:** `set` drops expired entries silently before it evicts for capacity.

I did not run a type check (`tsc --noEmit`), so the TypeScript strictness settings in `tsconfig.json` are unverified.
