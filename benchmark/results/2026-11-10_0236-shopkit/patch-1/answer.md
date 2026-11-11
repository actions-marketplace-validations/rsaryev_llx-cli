All six modules in `src/` are implemented as the README specifies, and `bun test` passes: 69 tests, 0 failures. I changed only the files in `src/`. The tests, `package.json` and `tsconfig.json` are untouched.

Choices the README left open:
- **Cart:** `remove(sku, qty)` with a `qty` larger than the line's quantity removes the whole line rather than throwing.
- **LRU:** `delete` on an expired key returns `false`. Expired entries are purged before capacity eviction, so they are dropped without calling `onEvict`.
- **Semver:** a purely numeric prerelease identifier with a leading zero (such as `1.0.0-01`) is rejected, following the "no leading zeros" rule.

I did not run a type check, since the task only asked for `bun test`.
