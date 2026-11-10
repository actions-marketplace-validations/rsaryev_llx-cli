I implemented all six modules in `src/` (`money`, `cart`, `csv`, `router`, `lru`, `semver`) as `README.md` specifies. `bun test` passes: 69 tests, 0 failures. I didn't touch the tests, `package.json` or `tsconfig.json`.

The README leaves some cases open, so I chose these:
- **Router:** adding `/users/:id` and `/users/:name` throws `RouterError`, because they match the same paths.
- **Cart:** `OFF0` is rejected as a malformed code.
- **Semver:** a range like `>1.2` means `>=1.3.0`, and `<=1.2` means `<1.3.0`.
