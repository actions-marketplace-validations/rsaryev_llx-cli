All six modules in `src/` are implemented as the README specifies, and `bun test` passes: 69 tests, 0 failures. I didn't touch the tests, `package.json` or `tsconfig.json`.

A few places where I had to choose, since the README doesn't say:

- **Router duplicates:** two patterns with the same shape but different param names (`/a/:id` and `/a/:name`) count as the same pattern and throw `RouterError`.
- **Cart tax:** the rounding uses floating-point arithmetic, so a fractional tax rate such as 8.25 could in principle land a cent off on an exact half. The tests don't hit this.
- **Semver:** ranges with an empty comparator set are treated as `*`. Partial versions with `^` or `~` (for example `^1.2`) are not supported and throw `SemverError`.
