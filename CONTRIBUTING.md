# Contributing

Thanks for helping. llx is meant to stay small and fast: five tools (`bash`, `apply_patch`,
`ask`, `agents`, `jobs`), synchronous code, no async runtime, and a release binary under
4 MB. A change that adds a dependency has to earn its size.

## Before you start

Read [ARCHITECTURE.md](ARCHITECTURE.md) and [AGENTS.md](AGENTS.md). ARCHITECTURE.md is binding:
it fixes the layers, what may know what, and every rule by id (L, C, P, A, K, W, B, R, S).
AGENTS.md says what each file is, with the behaviour and code rules. They apply to people as
much as to coding agents, and the tests in `apps/cli/tests/suite/repo/` check the layering and
the size budgets. A change that does not fit a rule is redesigned, or the rule is changed first,
in its own commit.

For a larger change, open an issue first to agree on the shape.

## Setup

The toolchain is the latest stable Rust (`rust-toolchain.toml` installs it with rustfmt and
clippy); the code also builds on the workspace's `rust-version` (1.88). Run `scripts/setup` once
per clone for the git hooks. Two checks need more than the toolchain: the site's build needs
`npm ci` in `site/` (it is skipped without it), and the coverage needs `cargo install
cargo-llvm-cov cargo-nextest`.

## Checks

One command runs the checks, the same ones CI runs (each CI step is `scripts/check --only NAME`):

```bash
scripts/setup          # once per clone: the git hooks below
scripts/check          # what your change needs: chosen from the files changed since main
scripts/check --fast   # the cheap part: format, lint of the crates changed, the repository's rules
scripts/check --all    # everything CI runs, in its order (add --coverage before a release)
scripts/check --list   # each check and the command it runs
```

It prints its plan first (which checks, and why), stops at the first failure with its output, and
ends with one line. After `scripts/setup`, a commit runs `scripts/check --fast --staged` on what
it stages (a staged file with unstaged edits is refused) and a push runs `scripts/check --fast` on
what is pushed; the tests, the size and the site run in CI, and by hand with `scripts/check` before
a change is called done. `--no-verify` skips the hooks. Agents in Claude Code run
`scripts/check --fast` when they end a turn (`.claude/settings.json`).

Under it are the usual commands, which you can still run alone: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings` (then, on libraries and binaries,
`-D unused_crate_dependencies`), `cargo test --workspace`, the release binary's size (under 5 MiB, warned above
4 MB), the site's build, the action's tests, and the linters: shellcheck, actionlint, the example
workflows' conventions and the Markdown's relative links, at pinned versions it downloads once.
Two checks CI does not run; run them yourself before a release ([RELEASING.md](RELEASING.md)):
the minimum Rust version (`cargo +1.88 check --workspace --all-targets --locked`) and the line
coverage, at least 85% (`scripts/check --all --coverage`, or `scripts/coverage.sh` alone).
The code stays under 50,000 lines of working code and 300 per file (ARCHITECTURE.md S1, S2).

## Code

- Small, focused modules: every file holds at most 300 lines of working code (blanks, comments
  and test files do not count), and one about to pass it is split by what it does first; rustfmt
  defaults.
- Doc comments and comments explain why, not what.
- Unit tests never live inline: `x.rs` ends with

  ```rust
  #[cfg(test)]
  #[path = "x_tests.rs"]
  mod tests;
  ```

  and the tests are in `x_tests.rs` next to it. The architecture test enforces it.
- Tests use fake ports, never the network.
- Exit codes and the `--json` schema are a public contract: change them only with a schema bump
  and a CHANGELOG entry.
- When behaviour changes, update together the `--help` text (the clap doc comments in
  `apps/server/src/cli.rs`), `README.md` and `CHANGELOG.md` under `## Unreleased`.

## Pull requests

Keep one change per pull request, with a description of what and why. Releases are cut by the
maintainers ([RELEASING.md](RELEASING.md)). By contributing you agree that your work is licensed
under the MIT license of this repository.
