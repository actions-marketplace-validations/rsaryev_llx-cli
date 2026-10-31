Implemented all six modules in `src/` according to the README. The acceptance suite passes: **69 tests, 0 failures**.

I also checked TypeScript. The earlier source type error is fixed; a full standalone `tsc` check is still blocked by the project’s ESNext-only configuration and missing `bun:test` declarations. Neither `package.json`, `tsconfig.json`, nor the tests were changed.
