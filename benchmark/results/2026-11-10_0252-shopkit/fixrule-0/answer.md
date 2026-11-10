All six modules in `src/` now implement the README, and `bun test` passes: 69 tests across 6 files, 0 failures. I changed only the `src/` files. The tests, `package.json` and `tsconfig.json` are untouched.

Some choices the README left open:

- **`Cart.remove` with a quantity above the line's quantity** removes the whole line rather than throwing.
- **`Cart.add`** rejects a negative or non-integer unit price with `CartError`. The README only specifies the quantity check.
- **`Router`** rejects a wildcard that is not the last segment, and rejects a duplicate pattern by its normalised form, so `/users/:id/` and `/users/:id` count as the same.
- **`parse` in `semver`** rejects a numeric prerelease identifier with a leading zero (`1.0.0-01`), following the SemVer rule.
- **`Lru`** drops expired entries silently during `set`, `size` and `keys`, so they never reach `onEvict`.

The `OFF` code is checked against the subtotal when it is applied. If the cart later shrinks, the discount is capped at the new subtotal instead of being rejected.
