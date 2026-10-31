## What and why

<!-- What changes, and the reason. Link the issue if there is one. -->

## Checks

- [ ] `scripts/check` passes (the checks this change needs: format, lints, tests, the repository's rules, size)
- [ ] Tests first: a bug has a test that failed before the fix, a feature has tests written from what a person does and sees
- [ ] ARCHITECTURE.md followed: new code is in its layer, and the rule ids the change touches are named below
- [ ] Behaviour changes: `--help` (`apps/server/src/cli.rs`), `README.md` and `CHANGELOG.md` (Unreleased), updated together

## Rules touched

<!-- Rule ids from ARCHITECTURE.md (L, C, P, A, K, W, B, R, S), or "none". -->
