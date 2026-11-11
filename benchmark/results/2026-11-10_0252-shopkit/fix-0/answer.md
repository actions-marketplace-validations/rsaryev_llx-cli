All six modules in `src/` are implemented as the README specifies, and `bun test` passes: 69 of 69. I changed only files in `src/`; the tests, `package.json` and `tsconfig.json` are untouched.

Two bugs came up during the run and are fixed:
- **semver:** a full version such as `1.2.3` was being read as a partial x-range, which broke `^`, `~` and `=` comparisons. Full versions now take the full-version path.
- **csv:** the error line for a bad record was one behind, because the line counter advanced after the record was ended. The counter now advances first.

A few spots the README leaves open, where I chose a behaviour:
- `Cart.remove` with a quantity larger than the line's quantity removes the whole line rather than throwing.
- `Lru.set` drops expired entries before it checks capacity, so an expired entry never causes a live one to be evicted.
- Semver ranges with a malformed comparator throw `SemverError`.
- A numeric prerelease identifier with a leading zero (such as `01`) is rejected, matching the rule for numbers in versions.

I did not run a type check (`tsc`); only `bun test` was run.
