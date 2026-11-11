All six modules in `src/` are implemented as the README specifies, and `bun test` passes: 69 tests across six files, 0 failures. I did not touch `test/`, `package.json` or `tsconfig.json`, and I did not commit anything.

Where the README was open to interpretation, I chose:

- **Cart `remove(sku, qty)`**: removing more than the line holds deletes the line. A non-positive or non-integer `qty` throws `CartError`.
- **Cart `OFF` codes**: the subtotal check runs when the code is applied. At discount time the discount is capped at the current subtotal, so later removals can still cap it.
- **Router wildcards**: `*name` only counts as a wildcard in the final position. Any other position is treated as a literal segment.
- **Semver ranges**: an empty comparator set matches any version. Malformed comparators throw `SemverError`. Partial versions with an operator (such as `>=1.2`) are parsed as strict versions, so `>=1.2` is rejected. Bare partials (`1.2`) and x-ranges work as specified.
- **Lru**: when over capacity, expired entries are purged silently before the least recent live entry is evicted.
