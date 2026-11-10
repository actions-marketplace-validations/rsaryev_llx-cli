All six modules in `src/` are implemented as the README specifies, and `bun test` passes: 69 tests across 6 files, 0 failures. I did not touch `test/`, `package.json`, or `tsconfig.json`.

The README leaves a few cases open, and I chose these behaviours:
- **`OFF0`** is rejected as a malformed code, since the README gives no meaning to a zero amount.
- **`Cart.remove(sku, qty)`** with a `qty` larger than the line's quantity removes the whole line rather than throwing.
- **`satisfies`** throws `SemverError` if the version argument is invalid, matching `parse`.
- **Router duplicates** are detected by the normalised pattern string, so `/users/:id` and `/users/:name` count as different patterns.
