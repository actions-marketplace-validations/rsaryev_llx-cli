# shellcheck shell=sh
# Which checks a change needs, chosen from its paths alone: `plan MODE COVERAGE` reads
# `path<TAB>present|gone` lines on stdin and prints `check<TAB>scope<TAB>why` lines in the one
# fixed order below. The same paths give the same plan: no clock, no network; the workspace's
# crates and their edges come from `cargo metadata --no-deps`, which reads the manifests only.
# Sourced by scripts/check; scripts/checks/check_test.sh holds the rules' examples.
#
# MODE is `fast` (the cheap subset, for hooks), `changed` (what a change needs) or `all` (every
# check, the whole workspace: what CI runs). Rust's scopes are cargo's: `-p` per crate, or
# `--workspace` when a file every crate reads changed (the root manifest, the lock, clippy.toml,
# the toolchain).

# Every check, in the order they run.
CHECKS="fmt clippy repo test coverage size site action check-test shellcheck actionlint examples mdlinks"

# The crate whose integration tests hold the repository's rules (apps/cli/tests/suite/repo).
REPO_CRATE=llx

# `dir<TAB>name<TAB>workspace deps` per crate, sorted by name. The deps are every
# workspace crate it depends on: normal, build and dev edges alike.
plan_crates() {
  plan_jq=$(tool jq) || return 1
  # shellcheck disable=SC2016 # jq's own variables
  cargo metadata --no-deps --format-version 1 --offline 2>/dev/null |
    "$plan_jq" -r '.workspace_root as $root | .packages[]
      | [(.manifest_path | ltrimstr($root + "/") | rtrimstr("/Cargo.toml")), .name,
         ([.dependencies[] | select(.path != null) | .name] | unique | join(" "))]
      | @tsv' | sort -t "$(printf '\t')" -k2,2
}

# `plan MODE COVERAGE [ONLY]`: ONLY names the checks of mode `only`. The table goes to awk on
# its stdin, the paths (this function's stdin) on fd 3. With no table (cargo or jq missing),
# every Rust change reads as the whole workspace's.
plan() {
  case $1 in
    all | only) plan_table= ;; # workspace-wide: no crate is named
    *) plan_table=$(plan_crates) || plan_table= ;;
  esac
  {
    printf '%s\n' "$plan_table" | awk -F '\t' -v mode="$1" -v coverage="$2" -v only="${3:-}" \
      -v repo_crate="$REPO_CRATE" -v order="$CHECKS" -f "${root:?}/scripts/checks/plan.awk" \
      - /dev/fd/3
  } 3<&0
}
