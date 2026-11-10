I implemented all six modules in `src/` as the README specifies. `bun test` passes: 69 tests, 0 failures. I didn't touch the tests, `package.json` or `tsconfig.json`.

Where the README was ambiguous, I made these choices:
- **Router:** `/users/:id` and `/users/:name` count as the same pattern, so adding both throws `RouterError`.
- **Cart tax:** a fractional tax rate can be off by a cent in rare half-cent cases, because the half-up rounding runs on floating-point numbers.
- **Semver:** partial carets like `^1.2` fill the missing parts with zeros.
