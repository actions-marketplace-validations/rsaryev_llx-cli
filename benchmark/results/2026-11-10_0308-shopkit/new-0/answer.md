All six modules in `src/` now follow the README, and `bun test` passes: 69 tests across 6 files, 0 failures. I did not change anything in `test/`, `package.json` or `tsconfig.json`.

- **`money.ts`**: parsing and formatting of USD and EUR amounts, plus `allocate`, which splits cents by ratios so the shares always sum to the total.
- **`cart.ts`**: lines, quantities and subtotal, with the `PCT` and `OFF` codes. A new code replaces the old one, and the discount is capped at the subtotal.
- **`csv.ts`**: an RFC 4180 parser and writer. Parse errors report the 1-based line where the bad record starts.
- **`router.ts`**: static, param and wildcard segments, query parsing, and the priority rule (static beats param beats wildcard, compared segment by segment).
- **`lru.ts`**: a least-recently-used cache with optional TTL and an injectable clock. When a set goes over capacity, expired entries are dropped silently before the least recent live entry is evicted.
- **`semver.ts`**: parsing, ordering with prereleases, and ranges (`||`, `^`, `~`, x-ranges, and the prerelease rule).

The type check I ran reports only `bun:test` import errors in the test files. Those come from my plain `tsc` run, which does not know Bun's types; I did not check the `src/` files under a Bun-aware type setup.
