# shellcheck shell=sh
# What each check runs: `run_check NAME SCOPE`, from the repository's top, in a subshell of
# scripts/check (so nothing here leaks into the next check). It passes with 0, is skipped with
# $SKIPPED (the reason printed), and fails with anything else. These commands are the ones CI
# runs (.github/workflows/ci.yml calls `scripts/check --only ...`): change them here, once.

SKIPPED=99
# The release binary's hard cap (5 MiB, ARCHITECTURE.md's Budgets), and the goal above which it
# warns (4 MB, AGENTS.md).
SIZE_LIMIT=5242880
SIZE_GOAL=4000000

run_check() {
  run_scope=$2
  "check_$(printf '%s' "$1" | tr '-' '_')"
}

# What a check runs, in a line (`scripts/check --list`).
describe() {
  case $1 in
    fmt) echo "cargo fmt --all --check" ;;
    clippy) echo "cargo clippy --all-targets -D warnings, then unused dependencies" ;;
    repo) echo "the repository's rules: cargo test -p $REPO_CRATE --test all -- suite::repo::" ;;
    test) echo "cargo test" ;;
    coverage) echo "scripts/coverage.sh: line coverage, at least 85% (needs cargo-llvm-cov)" ;;
    size) echo "the release build: llx fails above 5 MiB, warns above 4 MB" ;;
    site) echo "npm run build in site/: types, build, prerender (skipped without node_modules)" ;;
    action) echo "action/test.sh: the GitHub Action's script on a stub llx" ;;
    check-test) echo "scripts/checks/check_test.sh: the plan's examples, the link checker's" ;;
    shellcheck) echo "shellcheck (pinned) on every shell script" ;;
    actionlint) echo "actionlint (pinned) on the workflows and examples, the examples against action.yml" ;;
    examples) echo "the example workflows' schemas, time limits, models and versions" ;;
    mdlinks) echo "relative links and anchors in every Markdown file" ;;
  esac
}

# A line scripts/check shows under the check's verdict, even when its output is not shown.
note() {
  printf '%s\n' "$*" >>"${check_notes:?}"
}

check_fmt() { cargo fmt --all --check -- --color "${CARGO_TERM_COLOR:-auto}"; }
# A dependency no library or binary uses: test targets are left out, as each
# uses only some of its crate's dependencies and would name the rest. Workspace-wide: `--lib`
# on a scope of the binary alone finds no library, and the whole is quick once checked.
unused_dependencies() {
  cargo clippy --locked --workspace --lib --bins -- -D warnings -D unused_crate_dependencies
}
# shellcheck disable=SC2086 # a scope is cargo's words: `-p a -p b` or `--workspace`
check_clippy() {
  cargo clippy --locked --all-targets $run_scope -- -D warnings && unused_dependencies
}
check_repo() { cargo test --locked -p "$REPO_CRATE" --test all -- suite::repo::; }
# shellcheck disable=SC2086
check_test() { cargo test --locked $run_scope; }

check_coverage() {
  if ! cargo llvm-cov --version >/dev/null 2>&1 || ! cargo nextest --version >/dev/null 2>&1; then
    echo "coverage needs cargo-llvm-cov and cargo-nextest: cargo install cargo-llvm-cov cargo-nextest"
    return 1
  fi
  scripts/coverage.sh
}

# The release binary's size, gated.
check_size() {
  size_bin="${CARGO_TARGET_DIR:-target}/release/llx"
  cargo build --release --locked -p "$REPO_CRATE" || return 1
  size_default=$(wc -c <"$size_bin" | tr -d ' ')
  note "release llx on $(tool_platform): default $size_default B (fails above $SIZE_LIMIT, warns above $SIZE_GOAL)"
  if [ "$size_default" -gt "$SIZE_LIMIT" ]; then
    echo "the release binary is $size_default B, over its cap of $SIZE_LIMIT B (5 MiB):"
    echo "a dependency has to earn its size (AGENTS.md)"
    return 1
  fi
  if [ "$size_default" -gt "$SIZE_GOAL" ]; then
    note "warning: the release binary is $size_default B, past the $SIZE_GOAL B (4 MB) goal"
    if [ -n "${GITHUB_ACTIONS:-}" ]; then
      echo "::warning title=binary size::the release binary is $size_default B, past the $SIZE_GOAL B (4 MB) goal"
    fi
  fi
}

# `npm run build` type-checks first (tsc --noEmit), then builds and prerenders.
check_site() {
  if [ ! -d site/node_modules ]; then
    echo "site/node_modules is absent: run \`npm ci\` in site/ to check the site"
    return "$SKIPPED"
  fi
  (cd site && npm run --silent build)
}

check_action() { bash action/test.sh; }
check_check_test() { sh scripts/checks/check_test.sh; }
check_mdlinks() { sh scripts/checks/mdlinks.sh; }

# Every shell script of the repository, tracked or new.
shell_files() {
  git ls-files --cached --others --exclude-standard -- \
    '*.sh' scripts/check scripts/setup 'scripts/hooks/*' |
    while IFS= read -r shell_file; do
      if [ -f "$shell_file" ]; then printf '%s\n' "$shell_file"; fi
    done
}

check_shellcheck() {
  shellcheck_bin=$(tool shellcheck) || return 1
  shell_files | tr '\n' '\0' | xargs -0 "$shellcheck_bin" -x --
}

# The workflows and the examples, then the examples again as this checkout's action would run
# them (`uses: ./`), so each input and output they name is checked against action.yml. actionlint
# reads a local action from the project around the workflow: that is a scratch project holding
# action.yml and the rewritten examples, never this tree. Its `run:` scripts go through the
# pinned shellcheck, put first on PATH.
check_actionlint() {
  actionlint_bin=$(tool actionlint) && shellcheck_bin=$(tool shellcheck) || return 1
  PATH="$(dirname "$shellcheck_bin"):$PATH"
  export PATH
  "$actionlint_bin" .github/workflows/*.yml examples/github/*.yml || return 1
  actionlint_scratch=$(mktemp -d)
  mkdir -p "$actionlint_scratch/.git" "$actionlint_scratch/.github/workflows"
  cp action.yml "$actionlint_scratch/"
  for example in examples/github/*.yml; do
    sed 's#uses: rsaryev/llx@v0#uses: ./#' "$example" \
      >"$actionlint_scratch/.github/workflows/example-$(basename "$example")"
  done
  echo "the examples against action.yml (example-NAME.yml is examples/github/NAME.yml):"
  (cd "$actionlint_scratch" && "$actionlint_bin" .github/workflows/example-*.yml)
  actionlint_status=$?
  rm -rf "$actionlint_scratch"
  return "$actionlint_status"
}

# The examples' conventions: their schemas parse as JSON objects, every job has a time limit,
# every llx step names its model, and each action is used at one version in all of them.
check_examples() {
  yq_bin=$(tool yq) && jq_bin=$(tool jq) || return 1
  for example in examples/github/*.yml; do
    "$yq_bin" -o=json '[.jobs[].steps[] | select(.with.schema != null) | .with.schema]' "$example" |
      "$jq_bin" -e 'all(.[]; fromjson | type == "object")' >/dev/null ||
      { echo "$example: a schema is not a JSON object"; return 1; }
    "$yq_bin" -e '[.jobs[] | select(has("timeout-minutes") | not)] | length == 0' "$example" \
      >/dev/null || { echo "$example: a job has no timeout-minutes"; return 1; }
    "$yq_bin" -e '[.jobs[].steps[] | select(.uses == "rsaryev/llx@v0" and
      .with.install-only == null and .with.model == null)] | length == 0' "$example" \
      >/dev/null || { echo "$example: an llx step names no model"; return 1; }
  done
  examples_twice=$(grep -ho 'uses: [^ ]*@[^ ]*' examples/github/*.yml | sort -u |
    sed 's/@.*//' | uniq -d)
  if [ -n "$examples_twice" ]; then
    echo "used at more than one version: $examples_twice"
    return 1
  fi
}
