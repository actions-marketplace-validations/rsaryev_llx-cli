All six modules in `src/` are implemented as the README specifies, and `bun test` passes: 69 tests across 6 files, 0 failures. I changed only the files in `src/`; the tests, `package.json` and `tsconfig.json` are untouched.

- **`money.ts`**: `parseMoney` validates with one regex and returns integer cents. `formatMoney` handles USD and EUR, including negatives. `allocate` uses the floor-then-distribute rule and throws `MoneyError` on bad ratios.
- **`cart.ts`**: `Cart` keeps lines in first-add order. Percent discounts round half up in integer arithmetic. The `OFF` check runs when the code is applied, and `discount()` is capped at the current subtotal, so later removals are handled.
- **`csv.ts`**: a character-level RFC 4180 parser that tracks the line where each record starts for `CsvError.line`. `toCsv` quotes only when needed.
- **`router.ts`**: segment matching ranks each candidate route by its per-segment kinds (static, then param, then wildcard) and picks the lowest. Query and path values are decoded.
- **`lru.ts`**: a `Map`-based LRU with optional TTL. Expired entries are dropped silently, and `keys()` runs most-recent first.
- **`semver.ts`**: strict parsing with no leading zeros, prerelease comparison, and ranges covering comparators, `^`, `~`, x-ranges and `||`. The prerelease rule is applied per comparator set.

Choices the README does not settle:
- `cart.remove` with a quantity larger than the line's quantity throws `CartError`.
- A prerelease numeric identifier with a leading zero (for example `1.0.0-rc.01`) is rejected, following the semver spec.
- Partial versions in ranges (such as `^1.2` or `~1`) are read as x-ranges.
