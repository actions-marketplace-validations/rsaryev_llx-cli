Implemented all six modules in `src/` according to the README. The acceptance tests pass: **69 passed, 0 failed**. A strict type check of the source files and `git diff --check` also pass.

The full `tsc --noEmit` check reports only that `bun:test` declarations are unavailable for the test files in this environment; source files type-check cleanly.
