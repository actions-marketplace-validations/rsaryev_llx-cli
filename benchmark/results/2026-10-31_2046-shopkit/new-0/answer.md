Implemented all six modules in `src/` according to the README. The acceptance suite passes: **69 tests, 0 failures**.

I also tried `bunx tsc --noEmit`; it could not type-check the tests because the project lacks declarations for `bun:test`. I left `package.json`, `tsconfig.json`, and the fixed tests unchanged.
