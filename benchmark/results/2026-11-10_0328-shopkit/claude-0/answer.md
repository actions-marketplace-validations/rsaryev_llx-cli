All six modules in `src/` are implemented to the README, and `bun test` passes all 69 tests. I didn't touch the tests, `package.json` or `tsconfig.json`.

A few spec points were ambiguous, and I resolved them this way:
- **Router:** `/a/:id` and `/a/:name` count as the same pattern, so the second `add` doesn't throw. The second name is silently ignored, and only the first name appears in `params`. Only an exact repeat of the same segments throws `RouterError`.
- **Cart tax:** `total` rounds half up using floating-point arithmetic with a tiny epsilon. This is fine for ordinary rates, but an unusual fractional tax rate could be off by a cent.
- **Semver ranges:** a partial bound such as `>1.2` means `>=1.3.0`, and `<=1.2` means `<1.3.0`.
