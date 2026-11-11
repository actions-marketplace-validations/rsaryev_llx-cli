All six modules in `src/` now implement the README, and `bun test` passes: 69 of 69 tests across the six files. I changed only files under `src/`; the tests, `package.json` and `tsconfig.json` are untouched, and I did not commit.

What each module does, and where I had to make a call:

- **money.ts**: `parseMoney` validates the input with one regex, `formatMoney` handles USD and EUR grouping, and `allocate` uses the floor-then-distribute-remainder rule.
- **cart.ts**: lines live in a `Map`, so re-adding a removed sku moves it to the end, as the tests expect. An `OFF` code is checked against the subtotal when it is applied, and the discount is capped at the subtotal afterwards, so later removals still cap it. Percent discounts use integer half-up rounding.
- **csv.ts**: a character-level parser that tracks the line each record starts on, so errors report the right line.
- **router.ts**: routes are tried depth-first in static, then param, then wildcard order at each segment. That gives the required priority without comparing every match.
- **lru.ts**: a `Map` in recency order. Expired entries are pruned before the capacity check, so they never trigger `onEvict`.
- **semver.ts**: a strict version regex, identifier-by-identifier prerelease comparison, and range handling for `||`, the operators, `^`, `~` and x-ranges.

Choices the README leaves open:

- `cart.remove(sku, qty)` with `qty` greater than the line's quantity removes the whole line rather than throwing.
- `parse` rejects numeric prerelease identifiers with leading zeros (for example `1.0.0-01`), following the semver spec.
- `Router.add` treats two patterns as duplicates when their normalized segments are identical, so `/users/:id` and `/users/:uid` are not duplicates.
