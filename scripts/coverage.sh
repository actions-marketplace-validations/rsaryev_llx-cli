#!/bin/sh
# Line coverage of the whole workspace, every test run by cargo-nextest: fails under 85%; the floor is raised as coverage rises, never lowered.
# Needs cargo-llvm-cov and cargo-nextest (cargo install cargo-llvm-cov cargo-nextest).
# Extra arguments go to cargo llvm-cov, e.g. `scripts/coverage.sh --html` or `--lcov --output-path lcov.info`.
set -eu
cd "$(dirname "$0")/.."
exec cargo llvm-cov nextest --workspace --fail-under-lines 85 "$@"
