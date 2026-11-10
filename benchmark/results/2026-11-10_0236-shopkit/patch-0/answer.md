All six modules in `src/` now match the README, and `bun test` passes: 69 tests across 6 files, 0 failures. I changed only the files in `src/`; the tests, `package.json` and `tsconfig.json` are untouched.

- **`money.ts`**: parsing, USD/EUR formatting and `allocate` with the remainder handed out in order.
- **`cart.ts`**: lines keep first-add order, including after a line is removed and re-added. Percent discounts round half up, and the discount is recalculated from the current subtotal, so it never exceeds it.
- **`csv.ts`**: RFC 4180 parsing and writing. Errors carry the 1-based line where the bad record starts.
- **`router.ts`**: static, param and wildcard segments. When several patterns match, the ranking goes from the first segment on: static, then param, then wildcard.
- **`lru.ts`**: capacity-based eviction with `onEvict`, plus TTL measured from the last `set`. Expired entries are dropped silently.
- **`semver.ts`**: parsing, prerelease-aware comparison and ranges (`||`, comparators, `^`, `~`, x-ranges). A prerelease matches only when a comparator in its set names a prerelease with the same major.minor.patch.

The README leaves a few cases open, and I chose these:
- `cart.remove` with a quantity larger than the line's quantity throws `CartError`.
- `^` and `~` on partial versions such as `^1.2` or `~1` follow the usual npm-style bounds.
- A numeric prerelease identifier with a leading zero, such as `01`, is kept as a string.
