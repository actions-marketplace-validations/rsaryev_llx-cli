All six modules in `src/` now follow the README, and `bun test` passes: 69 of 69 tests across the six files. I did not change anything in `test/`, `package.json` or `tsconfig.json`.

The first run had one failure, in the router. It tried routes in insertion order, so `/users/:id` beat `/users/me`. The fix collects every route that matches and picks the one whose segments rank best from the first segment on (static, then param, then wildcard).

A few spots where the README left room, and what I chose:
- **`Cart.remove` with a quantity larger than the line:** the line is removed entirely rather than throwing.
- **`Cart` OFF codes:** the "subtotal at least twice the amount" check runs only when the code is applied. Later removals can leave the discount capped at the subtotal, which the README's "never more than the subtotal" rule allows.
- **Semver prerelease numbers:** a numeric prerelease identifier with a leading zero (such as `1.0.0-01`) is rejected, following the semver spec, though the README does not say so.
- **Semver operator with a space** (`>= 1.2.3`): accepted, since the tests don't cover it.
- **`Lru.size` and `keys()`:** both skip expired entries without removing them; only `get` and `set` purge them.
