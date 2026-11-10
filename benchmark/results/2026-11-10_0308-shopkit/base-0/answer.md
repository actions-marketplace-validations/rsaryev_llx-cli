All six modules in `src/` are implemented as the README specifies, and `bun test` passes: 69 tests, 0 failures. I did not touch `test/`, `package.json` or `tsconfig.json`.

What each module does, briefly:

- **money.ts**: `parseMoney` accepts the README's amount format and throws `MoneyError` otherwise. `formatMoney` handles USD and EUR, including negatives. `allocate` floors each share, then hands out the leftover cents in order to the positive ratios.
- **cart.ts**: `Cart` keeps lines in first-add order and drops a line when its quantity reaches 0. `PCT` and `OFF` codes are checked when applied. Percent discounts round half up, and the discount is capped at the subtotal.
- **csv.ts**: `parseCsv` is a small state machine that tracks the starting line of each record for `CsvError.line`. `toCsv` quotes only when needed.
- **router.ts**: static, param and wildcard segments are ranked per segment, so the most specific match wins. Query strings are decoded, with `+` read as a space.
- **lru.ts**: a `Map` keeps recency order. TTL expiry is checked lazily, and expired entries are removed without calling `onEvict`.
- **semver.ts**: `parse` and `compare` follow the semver prerelease rules. `satisfies` expands each comparator, including `^`, `~` and x-ranges, and applies the prerelease gate per set.

Some choices the README left open:

- A purely numeric prerelease identifier with a leading zero (such as `1.0.0-01`) is rejected, following the semver spec.
- When an LRU is over capacity, it drops expired entries silently before evicting live ones.
- Partial ranges with `^` or `~` (such as `^0.2` or `~1`) are read the usual npm way.

I did not run `tsc`, since the task only asked for `bun test`.
